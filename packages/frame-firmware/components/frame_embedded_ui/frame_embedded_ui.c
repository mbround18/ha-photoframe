#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "freertos/FreeRTOS.h"
#include "freertos/semphr.h"
#include "bsp/display.h"
#include "esp_check.h"
#include "esp_err.h"
#include "esp_heap_caps.h"
#include "driver/ledc.h"
#include "esp_lcd_jd9365.h"
#include "esp_lcd_mipi_dsi.h"
#include "esp_lcd_panel_ops.h"
#include "esp_ldo_regulator.h"
#include "esp_log.h"
#include "esp_task_wdt.h"
#include "frame_embedded_ui.h"
#include "bsp/esp32_p4_function_ev_board.h"

static const char *TAG = "frame_embedded_ui";

/*
 * This board carries a JD9365-driven 800x1280 panel. The upstream
 * esp32_p4_function_ev_board BSP installs an ILI9881C for
 * CONFIG_BSP_LCD_TYPE_1280_800, which this panel does not answer -- it reports
 * ID1/ID2/ID3 all 0x0 and never receives a valid init sequence. So we bring the
 * DSI bus and panel up ourselves, following the vendor's own video_lcd_display
 * demo. See docs/Hardware-Reference.md sections 2 and 6.
 */
#define FRAME_LCD_RST_GPIO      (27)  /* LCD_RST  */
#define FRAME_LCD_BACKLIGHT_GPIO (23) /* LCD_PWM -> MP3202 boost EN */
#define FRAME_MIPI_DSI_LDO_CHAN (3)
#define FRAME_MIPI_DSI_LDO_MV   (2500)

static esp_ldo_channel_handle_t s_mipi_phy_pwr;
static esp_lcd_dsi_bus_handle_t s_dsi_bus;
static esp_lcd_panel_io_handle_t s_dbi_io;
static bool s_started;
static bool s_panel_started;
static bool s_panel_rotation_in_software;
static bsp_lcd_handles_t s_panel_handles;
static uint16_t s_panel_width;
static uint16_t s_panel_height;
static uint16_t s_panel_rotation_degrees;
static uint16_t s_panel_draw_width;
static uint16_t s_panel_draw_height;
static uint16_t *s_rotated_framebuffer;
static uint32_t s_panel_present_count;
static SemaphoreHandle_t s_panel_mutex;
static lv_obj_t *s_phase_badge;
static lv_obj_t *s_headline;
static lv_obj_t *s_status;
static lv_obj_t *s_detail;
static lv_obj_t *s_step_cards[4];
static lv_obj_t *s_step_titles[4];
static lv_obj_t *s_step_bodies[4];
static lv_obj_t *s_connection_value;
static lv_obj_t *s_connection_hint;
static lv_obj_t *s_action_title;
static lv_obj_t *s_action_value;
static lv_obj_t *s_action_hint;
static lv_obj_t *s_tile_titles[4];
static lv_obj_t *s_tile_values[4];
static lv_obj_t *s_tile_hints[4];

static esp_err_t lock_display(void)
{
  if (s_panel_mutex == NULL)
  {
    s_panel_mutex = xSemaphoreCreateMutex();
  }

  ESP_RETURN_ON_FALSE(s_panel_mutex != NULL, ESP_ERR_NO_MEM, TAG, "panel mutex allocation failed");

  TickType_t timeout_ticks = pdMS_TO_TICKS(1000);
  return xSemaphoreTake(s_panel_mutex, timeout_ticks) == pdTRUE ? ESP_OK : ESP_ERR_TIMEOUT;
}

static void unlock_display(void)
{
  if (s_panel_mutex != NULL)
  {
    xSemaphoreGive(s_panel_mutex);
  }
}

/*
 * The backlight is an MP3202 boost converter whose EN pin is driven from
 * GPIO23 (net LCD_PWM). Drive it with LEDC so brightness is available later
 * (FR-034) rather than only on/off.
 */
static esp_err_t configure_backlight(void)
{
  const ledc_timer_config_t timer = {
      .speed_mode = LEDC_LOW_SPEED_MODE,
      .duty_resolution = LEDC_TIMER_10_BIT,
      .timer_num = LEDC_TIMER_1,
      .freq_hz = 5000,
      .clk_cfg = LEDC_AUTO_CLK,
  };
  ESP_RETURN_ON_ERROR(ledc_timer_config(&timer), TAG, "backlight timer config failed");

  const ledc_channel_config_t channel = {
      .gpio_num = FRAME_LCD_BACKLIGHT_GPIO,
      .speed_mode = LEDC_LOW_SPEED_MODE,
      .channel = LEDC_CHANNEL_1,
      .timer_sel = LEDC_TIMER_1,
      .duty = 0,
      .hpoint = 0,
  };
  ESP_RETURN_ON_ERROR(ledc_channel_config(&channel), TAG, "backlight channel config failed");

  return ESP_OK;
}

esp_err_t frame_embedded_panel_set_brightness(uint8_t percent)
{
  if (percent > 100)
  {
    percent = 100;
  }

  const uint32_t duty = (uint32_t)((1023U * percent) / 100U);
  ESP_RETURN_ON_ERROR(ledc_set_duty(LEDC_LOW_SPEED_MODE, LEDC_CHANNEL_1, duty), TAG,
                      "backlight set_duty failed");
  return ledc_update_duty(LEDC_LOW_SPEED_MODE, LEDC_CHANNEL_1);
}

static esp_err_t enable_vendor_backlight(void)
{
  return frame_embedded_panel_set_brightness(100);
}

/*
 * Bring up the MIPI-DSI bus and the JD9365 panel directly.
 *
 * Order matters: the DSI PHY is powered from an internal LDO channel and must
 * be up before the bus is created, or the bus init fails.
 */
static esp_err_t panel_init_jd9365(void)
{
  esp_ldo_channel_config_t ldo_cfg = {
      .chan_id = FRAME_MIPI_DSI_LDO_CHAN,
      .voltage_mv = FRAME_MIPI_DSI_LDO_MV,
  };
  ESP_RETURN_ON_ERROR(esp_ldo_acquire_channel(&ldo_cfg, &s_mipi_phy_pwr), TAG,
                      "MIPI DSI PHY LDO acquire failed");
  ESP_LOGI(TAG, "MIPI DSI PHY powered from LDO channel %d at %d mV",
           FRAME_MIPI_DSI_LDO_CHAN, FRAME_MIPI_DSI_LDO_MV);

  esp_lcd_dsi_bus_config_t bus_config = JD9365_PANEL_BUS_DSI_2CH_CONFIG();
  ESP_RETURN_ON_ERROR(esp_lcd_new_dsi_bus(&bus_config, &s_dsi_bus), TAG,
                      "DSI bus creation failed");

  esp_lcd_dbi_io_config_t dbi_config = JD9365_PANEL_IO_DBI_CONFIG();
  ESP_RETURN_ON_ERROR(esp_lcd_new_panel_io_dbi(s_dsi_bus, &dbi_config, &s_dbi_io), TAG,
                      "DBI panel IO creation failed");

  esp_lcd_dpi_panel_config_t dpi_config =
      JD9365_800_1280_PANEL_60HZ_DPI_CONFIG(LCD_COLOR_PIXEL_FORMAT_RGB565);
  dpi_config.num_fbs = 1;

  jd9365_vendor_config_t vendor_config = {
      .mipi_config = {
          .dsi_bus = s_dsi_bus,
          .dpi_config = &dpi_config,
      },
  };

  const esp_lcd_panel_dev_config_t panel_config = {
      .reset_gpio_num = FRAME_LCD_RST_GPIO,
      .rgb_ele_order = LCD_RGB_ELEMENT_ORDER_RGB,
      .bits_per_pixel = 16,
      .vendor_config = &vendor_config,
  };

  esp_lcd_panel_handle_t panel = NULL;
  ESP_RETURN_ON_ERROR(esp_lcd_new_panel_jd9365(s_dbi_io, &panel_config, &panel), TAG,
                      "JD9365 panel creation failed");
  ESP_RETURN_ON_ERROR(esp_lcd_panel_reset(panel), TAG, "JD9365 panel reset failed");
  ESP_RETURN_ON_ERROR(esp_lcd_panel_init(panel), TAG, "JD9365 panel init failed");

  s_panel_handles.panel = panel;
  s_panel_handles.control = NULL;

  ESP_LOGI(TAG, "JD9365 panel initialized (800x1280 native, 2 DSI lanes)");
  return ESP_OK;
}

static esp_lcd_panel_handle_t panel_control_handle(void)
{
  return s_panel_handles.control ? s_panel_handles.control : s_panel_handles.panel;
}

static esp_err_t apply_panel_rotation(uint16_t rotation_degrees)
{
  bool swap_xy = false;
  bool mirror_x = true;
  bool mirror_y = true;

  switch (rotation_degrees)
  {
  case 0:
    swap_xy = false;
    mirror_x = true;
    mirror_y = true;
    break;
  case 90:
    swap_xy = true;
    mirror_x = true;
    mirror_y = false;
    break;
  case 180:
    swap_xy = false;
    mirror_x = false;
    mirror_y = false;
    break;
  case 270:
    swap_xy = true;
    mirror_x = false;
    mirror_y = true;
    break;
  default:
    ESP_LOGE(TAG, "unsupported panel rotation: %u", rotation_degrees);
    return ESP_ERR_INVALID_ARG;
  }

  esp_lcd_panel_handle_t control_handle = panel_control_handle();
  ESP_RETURN_ON_FALSE(control_handle != NULL, ESP_ERR_INVALID_STATE, TAG, "panel control handle missing");

  esp_err_t swap_result = esp_lcd_panel_swap_xy(control_handle, swap_xy);
  if (swap_result == ESP_ERR_NOT_SUPPORTED && swap_xy)
  {
    s_panel_rotation_in_software = true;
    swap_xy = false;
    mirror_x = true;
    mirror_y = true;
    ESP_LOGW(TAG,
             "panel does not support swap_xy; using software rotation fallback for %u degrees",
             rotation_degrees);
  }
  else
  {
    ESP_RETURN_ON_ERROR(swap_result, TAG, "panel swap_xy failed");
    s_panel_rotation_in_software = false;
  }

  ESP_RETURN_ON_ERROR(esp_lcd_panel_mirror(control_handle, mirror_x, mirror_y), TAG, "panel mirror failed");
  return ESP_OK;
}

static esp_err_t ensure_rotated_framebuffer(size_t pixel_count)
{
  if (s_rotated_framebuffer != NULL)
  {
    return ESP_OK;
  }

  s_rotated_framebuffer = heap_caps_malloc(pixel_count * sizeof(uint16_t), MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
  if (s_rotated_framebuffer == NULL)
  {
    s_rotated_framebuffer = malloc(pixel_count * sizeof(uint16_t));
  }

  ESP_RETURN_ON_FALSE(s_rotated_framebuffer != NULL, ESP_ERR_NO_MEM, TAG, "rotated framebuffer allocation failed");
  return ESP_OK;
}

static inline void service_task_watchdog(void)
{
  if (esp_task_wdt_status(NULL) == ESP_OK)
  {
    (void)esp_task_wdt_reset();
  }
}

static void rotate_rgb565_framebuffer(const uint16_t *src,
                                      uint16_t src_width,
                                      uint16_t src_height,
                                      uint16_t rotation_degrees,
                                      uint16_t *dst)
{
  if (rotation_degrees == 270)
  {
    for (uint16_t y = 0; y < src_height; ++y)
    {
      if ((y & 0x0fU) == 0)
      {
        service_task_watchdog();
      }
      for (uint16_t x = 0; x < src_width; ++x)
      {
        uint16_t dst_x = y;
        uint16_t dst_y = (uint16_t)(src_width - 1 - x);
        dst[(size_t)dst_y * src_height + dst_x] = src[(size_t)y * src_width + x];
      }
    }
    return;
  }

  if (rotation_degrees == 90)
  {
    for (uint16_t y = 0; y < src_height; ++y)
    {
      if ((y & 0x0fU) == 0)
      {
        service_task_watchdog();
      }
      for (uint16_t x = 0; x < src_width; ++x)
      {
        uint16_t dst_x = (uint16_t)(src_height - 1 - y);
        uint16_t dst_y = x;
        dst[(size_t)dst_y * src_height + dst_x] = src[(size_t)y * src_width + x];
      }
    }
    return;
  }

  if (rotation_degrees == 180)
  {
    for (uint16_t y = 0; y < src_height; ++y)
    {
      if ((y & 0x0fU) == 0)
      {
        service_task_watchdog();
      }
      for (uint16_t x = 0; x < src_width; ++x)
      {
        uint16_t dst_x = (uint16_t)(src_width - 1 - x);
        uint16_t dst_y = (uint16_t)(src_height - 1 - y);
        dst[(size_t)dst_y * src_width + dst_x] = src[(size_t)y * src_width + x];
      }
    }
    return;
  }

  memcpy(dst, src, (size_t)src_width * src_height * sizeof(uint16_t));
}

esp_err_t frame_embedded_panel_init(uint16_t width,
                                    uint16_t height,
                                    uint16_t rotation_degrees)
{
  if (s_panel_started)
  {
    return ESP_OK;
  }

  memset(&s_panel_handles, 0, sizeof(s_panel_handles));

  ESP_RETURN_ON_ERROR(panel_init_jd9365(), TAG, "JD9365 display init failed");
  ESP_RETURN_ON_ERROR(configure_backlight(), TAG, "backlight configuration failed");
  ESP_RETURN_ON_ERROR(enable_vendor_backlight(), TAG, "backlight enable failed");
  ESP_RETURN_ON_ERROR(apply_panel_rotation(rotation_degrees), TAG, "panel rotation setup failed");

  s_panel_width = width;
  s_panel_height = height;
  s_panel_rotation_degrees = rotation_degrees;
  s_panel_draw_width = s_panel_rotation_in_software ? height : width;
  s_panel_draw_height = s_panel_rotation_in_software ? width : height;

  if (s_panel_rotation_in_software)
  {
    ESP_RETURN_ON_ERROR(ensure_rotated_framebuffer((size_t)s_panel_draw_width * s_panel_draw_height),
                        TAG,
                        "rotated framebuffer setup failed");
  }

  s_panel_present_count = 0;
  s_panel_started = true;

  ESP_LOGI(TAG,
           "raw panel initialized for %ux%u at rotation %u (draw %ux%u, software_rotation=%s)",
           width,
           height,
           rotation_degrees,
           s_panel_draw_width,
           s_panel_draw_height,
           s_panel_rotation_in_software ? "true" : "false");

  return ESP_OK;
}

esp_err_t frame_embedded_panel_present(const uint16_t *pixels,
                                       uint16_t width,
                                       uint16_t height)
{
  ESP_RETURN_ON_FALSE(s_panel_started, ESP_ERR_INVALID_STATE, TAG, "raw panel not initialized");
  ESP_RETURN_ON_FALSE(pixels != NULL, ESP_ERR_INVALID_ARG, TAG, "pixel buffer missing");

  if (width != s_panel_width || height != s_panel_height)
  {
    ESP_LOGW(TAG,
             "present size changed from %ux%u to %ux%u",
             s_panel_width,
             s_panel_height,
             width,
             height);
    s_panel_width = width;
    s_panel_height = height;
    s_panel_draw_width = s_panel_rotation_in_software ? height : width;
    s_panel_draw_height = s_panel_rotation_in_software ? width : height;
  }

  const uint16_t *draw_pixels = pixels;
  if (s_panel_rotation_in_software)
  {
    ESP_RETURN_ON_ERROR(ensure_rotated_framebuffer((size_t)s_panel_draw_width * s_panel_draw_height),
                        TAG,
                        "rotated framebuffer setup failed");
    service_task_watchdog();
    rotate_rgb565_framebuffer(pixels,
                              width,
                              height,
                              s_panel_rotation_degrees,
                              s_rotated_framebuffer);
    draw_pixels = s_rotated_framebuffer;
  }

  ESP_RETURN_ON_ERROR(lock_display(), TAG, "display lock failed");
  service_task_watchdog();
  esp_err_t draw_result = esp_lcd_panel_draw_bitmap(s_panel_handles.panel,
                                                    0,
                                                    0,
                                                    s_panel_draw_width,
                                                    s_panel_draw_height,
                                                    draw_pixels);
  unlock_display();
  ESP_RETURN_ON_ERROR(draw_result, TAG, "panel draw bitmap failed");
  service_task_watchdog();

  s_panel_present_count += 1;
  if (s_panel_present_count == 1 || (s_panel_present_count % 120U) == 0)
  {
    ESP_LOGI(TAG,
             "panel present complete: frame=%lu draw=%ux%u rotation=%u software_rotation=%s",
             (unsigned long)s_panel_present_count,
             s_panel_draw_width,
             s_panel_draw_height,
             s_panel_rotation_degrees,
             s_panel_rotation_in_software ? "true" : "false");
  }

  return ESP_OK;
}

static bool text_present(const char *value)
{
  return value != NULL && value[0] != '\0';
}

static bool str_eq(const char *value, const char *expected)
{
  return text_present(value) && strcmp(value, expected) == 0;
}

static bool browser_pairing_ready(const char *local_setup_url,
                                  const char *local_setup_ip_url,
                                  const char *pairing_code,
                                  const char *auth_user_code)
{
  return text_present(pairing_code) && !text_present(auth_user_code) &&
         (text_present(local_setup_url) || text_present(local_setup_ip_url));
}

static int current_step(const char *phase,
                        const char *network,
                        const char *local_setup_url,
                        const char *local_setup_ip_url,
                        const char *pairing_code,
                        const char *auth_user_code)
{
  if (str_eq(phase, "Ready"))
  {
    return 4;
  }

  if (browser_pairing_ready(local_setup_url, local_setup_ip_url, pairing_code, auth_user_code))
  {
    return 3;
  }

  if (str_eq(network, "Authorizing"))
  {
    return 3;
  }

  if (str_eq(network, "Provisioning"))
  {
    return 2;
  }

  if (str_eq(phase, "Setup"))
  {
    return 1;
  }

  return 0;
}

static lv_obj_t *create_card(lv_obj_t *parent,
                             lv_coord_t x,
                             lv_coord_t y,
                             lv_coord_t width,
                             lv_coord_t height,
                             uint32_t background,
                             uint32_t border)
{
  lv_obj_t *card = lv_obj_create(parent);
  lv_obj_remove_style_all(card);
  lv_obj_set_pos(card, x, y);
  lv_obj_set_size(card, width, height);
  lv_obj_set_style_radius(card, 24, 0);
  lv_obj_set_style_bg_color(card, lv_color_hex(background), 0);
  lv_obj_set_style_bg_opa(card, LV_OPA_COVER, 0);
  lv_obj_set_style_border_color(card, lv_color_hex(border), 0);
  lv_obj_set_style_border_width(card, 1, 0);
  lv_obj_set_style_border_opa(card, LV_OPA_COVER, 0);
  lv_obj_set_style_pad_all(card, 0, 0);
  lv_obj_clear_flag(card, LV_OBJ_FLAG_SCROLLABLE);
  return card;
}

static lv_obj_t *create_label(lv_obj_t *parent,
                              lv_coord_t x,
                              lv_coord_t y,
                              lv_coord_t width,
                              uint32_t color,
                              lv_text_align_t align,
                              bool wrap)
{
  lv_obj_t *label = lv_label_create(parent);
  lv_obj_set_pos(label, x, y);
  lv_obj_set_width(label, width);
  if (wrap)
  {
    lv_label_set_long_mode(label, LV_LABEL_LONG_WRAP);
  }
  lv_obj_set_style_text_color(label, lv_color_hex(color), 0);
  lv_obj_set_style_text_align(label, align, 0);
  return label;
}

static void set_step_style(size_t index, bool active, bool complete)
{
  uint32_t background = 0x0F1728;
  uint32_t border = 0x1F2A3D;
  uint32_t body_color = 0x94A3B8;

  if (complete)
  {
    background = 0x10281F;
    border = 0x34D399;
  }
  else if (active)
  {
    background = 0x14304A;
    border = 0x7DD3FC;
    body_color = 0xDBEAFE;
  }

  lv_obj_set_style_bg_color(s_step_cards[index], lv_color_hex(background), 0);
  lv_obj_set_style_border_color(s_step_cards[index], lv_color_hex(border), 0);
  lv_obj_set_style_text_color(s_step_titles[index], lv_color_hex(0xF8FAFC), 0);
  lv_obj_set_style_text_color(s_step_bodies[index], lv_color_hex(body_color), 0);
}

static void set_tile(size_t index, const char *title, const char *value, const char *hint)
{
  lv_label_set_text(s_tile_titles[index], title);
  lv_label_set_text(s_tile_values[index], value);
  lv_label_set_text(s_tile_hints[index], hint);
}

esp_err_t frame_embedded_ui_start(void)
{
  if (s_started)
  {
    return ESP_OK;
  }

  bsp_display_cfg_t display_cfg = {
      .lvgl_port_cfg = ESP_LVGL_PORT_INIT_CONFIG(),
      .buffer_size = BSP_LCD_DRAW_BUFF_SIZE,
      .double_buffer = BSP_LCD_DRAW_BUFF_DOUBLE,
      .flags = {
#if CONFIG_BSP_LCD_COLOR_FORMAT_RGB888
          .buff_dma = false,
#else
          .buff_dma = true,
#endif
          .buff_spiram = true,
          .sw_rotate = true,
      },
  };

  lv_display_t *display = bsp_display_start_with_config(&display_cfg);
  ESP_RETURN_ON_FALSE(display != NULL, ESP_FAIL, TAG, "display register failed");
  ESP_RETURN_ON_ERROR(enable_vendor_backlight(), TAG, "vendor backlight enable failed");
  ESP_RETURN_ON_ERROR(lock_display(), TAG, "display lock failed");

  bsp_display_rotate(display, LV_DISPLAY_ROTATION_270);

  lv_obj_t *screen = lv_screen_active();
  lv_obj_set_style_bg_color(screen, lv_color_hex(0x09111D), 0);
  lv_obj_set_style_bg_opa(screen, LV_OPA_COVER, 0);

  lv_obj_t *left_panel = create_card(screen, 32, 32, 340, 736, 0x0D1728, 0x203147);
  lv_obj_t *right_panel = create_card(screen, 388, 32, 860, 736, 0x0C1423, 0x203147);

  lv_obj_t *progress_eyebrow = create_label(left_panel, 24, 24, 280, 0xE2E8F0, LV_TEXT_ALIGN_LEFT, false);
  lv_label_set_text(progress_eyebrow, "Photo Frame");

  lv_obj_t *progress_title = create_label(left_panel, 24, 54, 290, 0xF8FAFC, LV_TEXT_ALIGN_LEFT, false);
  lv_label_set_text(progress_title, "Setup progress");

  lv_obj_t *progress_body = create_label(left_panel, 24, 88, 292, 0x94A3B8, LV_TEXT_ALIGN_LEFT, true);
  lv_label_set_text(progress_body, "Each step becomes clearer as the frame learns your network and account details.");

  static const char *step_titles[4] = {
      "1  Wake up",
      "2  Connect Wi-Fi",
      "3  Pair and sign in",
      "4  Enjoy photos",
  };

  static const char *step_bodies[4] = {
      "Power, display, and setup services start here.",
      "Join the temporary setup network so the frame can reach your home internet.",
      "Pair a nearby browser, then approve Google access with the on-screen code.",
      "The frame finishes setup and begins pulling in your library.",
  };

  for (size_t i = 0; i < 4; ++i)
  {
    lv_coord_t top = 148 + (lv_coord_t)(i * 104);
    s_step_cards[i] = create_card(left_panel, 20, top, 300, 90, 0x0F1728, 0x1F2A3D);
    s_step_titles[i] = create_label(s_step_cards[i], 18, 16, 264, 0xF8FAFC, LV_TEXT_ALIGN_LEFT, false);
    s_step_bodies[i] = create_label(s_step_cards[i], 18, 44, 264, 0x94A3B8, LV_TEXT_ALIGN_LEFT, true);
    lv_label_set_text(s_step_titles[i], step_titles[i]);
    lv_label_set_text(s_step_bodies[i], step_bodies[i]);
  }

  lv_obj_t *connection_card = create_card(left_panel, 20, 576, 300, 138, 0x10192A, 0x26364B);
  lv_obj_t *connection_title = create_label(connection_card, 18, 16, 240, 0x7DD3FC, LV_TEXT_ALIGN_LEFT, false);
  lv_label_set_text(connection_title, "Connection");
  s_connection_value = create_label(connection_card, 18, 44, 240, 0xF8FAFC, LV_TEXT_ALIGN_LEFT, false);
  s_connection_hint = create_label(connection_card, 18, 76, 264, 0x94A3B8, LV_TEXT_ALIGN_LEFT, true);

  lv_obj_t *hero_card = create_card(right_panel, 24, 24, 812, 214, 0x10253A, 0x27415D);
  lv_obj_t *badge_card = create_card(hero_card, 24, 20, 150, 34, 0x153958, 0x153958);
  s_phase_badge = create_label(badge_card, 0, 8, 150, 0xDBEAFE, LV_TEXT_ALIGN_CENTER, false);
  s_headline = create_label(hero_card, 24, 68, 764, 0xF8FAFC, LV_TEXT_ALIGN_LEFT, true);
  s_status = create_label(hero_card, 24, 116, 764, 0xDBE4F0, LV_TEXT_ALIGN_LEFT, true);
  s_detail = create_label(hero_card, 24, 160, 764, 0x9FB2C7, LV_TEXT_ALIGN_LEFT, true);

  lv_obj_t *action_card = create_card(right_panel, 24, 254, 812, 130, 0x132033, 0x264057);
  s_action_title = create_label(action_card, 24, 18, 220, 0x7DD3FC, LV_TEXT_ALIGN_LEFT, false);
  s_action_value = create_label(action_card, 24, 48, 764, 0xF8FAFC, LV_TEXT_ALIGN_LEFT, true);
  s_action_hint = create_label(action_card, 24, 90, 764, 0xDBE4F0, LV_TEXT_ALIGN_LEFT, true);

  lv_coord_t tile_x[4] = {24, 430, 24, 430};
  lv_coord_t tile_y[4] = {400, 400, 558, 558};
  for (size_t i = 0; i < 4; ++i)
  {
    lv_obj_t *tile = create_card(right_panel, tile_x[i], tile_y[i], 406, 146, 0x10192A, 0x243247);
    s_tile_titles[i] = create_label(tile, 20, 18, 320, 0x7DD3FC, LV_TEXT_ALIGN_LEFT, false);
    s_tile_values[i] = create_label(tile, 20, 46, 356, 0xF8FAFC, LV_TEXT_ALIGN_LEFT, true);
    s_tile_hints[i] = create_label(tile, 20, 94, 356, 0x9FB2C7, LV_TEXT_ALIGN_LEFT, true);
  }

  lv_label_set_text(s_phase_badge, "Starting up");
  lv_label_set_text(s_headline, "Welcome home");
  lv_label_set_text(s_status, "Booting up the display and preparing setup.");
  lv_label_set_text(s_detail, "We\'re starting services, checking connectivity, and getting your frame ready for first-time setup.");
  lv_label_set_text(s_connection_value, "Unprovisioned");
  lv_label_set_text(s_connection_hint, "Current network details will appear here.");
  lv_label_set_text(s_action_title, "Current status");
  lv_label_set_text(s_action_value, "Starting up");
  lv_label_set_text(s_action_hint, "The frame will guide you through setup as soon as it is ready.");
  set_tile(0, "What happens next", "The device will guide you into setup.", "No action is needed until the frame shows a setup network or sign-in code.");
  set_tile(1, "Quick access", "Waiting for local setup link", "A direct local link appears here when available.");
  set_tile(2, "Account", "Not signed in yet", "Google sign-in happens after Wi-Fi is connected.");
  set_tile(3, "State summary", "Welcome home", "Preparing guided setup.");

  unlock_display();
  s_started = true;
  ESP_LOGI(TAG, "embedded display started");

  return ESP_OK;
}

esp_err_t frame_embedded_ui_sync(const char *phase,
                                 const char *headline,
                                 const char *status,
                                 const char *network,
                                 const char *detail,
                                 const char *owner_email,
                                 const char *provisioning_ssid,
                                 const char *provisioning_password,
                                 const char *local_setup_url,
                                 const char *local_setup_ip_url,
                                 const char *pairing_code,
                                 const char *auth_user_code,
                                 const char *auth_verification_uri)
{
  ESP_RETURN_ON_FALSE(s_started, ESP_ERR_INVALID_STATE, TAG, "display not started");
  ESP_RETURN_ON_ERROR(lock_display(), TAG, "display lock failed");

  bool pairing_browser = browser_pairing_ready(local_setup_url, local_setup_ip_url, pairing_code, auth_user_code);
  int step = current_step(phase, network, local_setup_url, local_setup_ip_url, pairing_code, auth_user_code);
  const char *badge = "Setup status";
  const char *action_title = "Current status";
  const char *action_value = text_present(status) ? status : "Starting up";
  const char *action_hint = text_present(detail) ? detail : "The frame will guide you through setup.";
  const char *next_value = "The device will guide you into setup as soon as connectivity is ready.";
  const char *next_hint = "No action is needed until the frame shows a setup network or sign-in code.";
  const char *access_title = "Quick access";
  const char *access_value = "Waiting for local setup link";
  const char *access_hint = "A direct local link appears here when available.";
  const char *account_value = text_present(owner_email) ? owner_email : "Not signed in yet";
  const char *account_hint = text_present(owner_email)
                                 ? "The frame will keep using this account until you re-pair it."
                                 : "Google sign-in happens after Wi-Fi is connected.";

  if (step == 4)
  {
    badge = "All set";
    action_title = "Owner";
    action_value = text_present(owner_email) ? owner_email : "Signed in";
    action_hint = text_present(local_setup_url)
                      ? local_setup_url
                      : "Your photos should begin appearing here shortly.";
    next_value = "Sit back and let the frame pull in your photos.";
    next_hint = "You can return to local setup later if you need to reconnect or troubleshoot.";
  }
  else if (pairing_browser)
  {
    badge = "Browser pairing";
    action_title = "Pass code";
    action_value = text_present(pairing_code) ? pairing_code : "Waiting for pass code";
    action_hint = "Use this pass code to validate the browser with the frame. Google sign-in stays hidden until that validation is complete.";
    next_value = "Enter the pass code in your browser, then the sign-in page and Google code will appear.";
    next_hint = "This prevents someone on the network from opening setup unless they can also see the code on the frame.";
  }
  else if (step == 3)
  {
    badge = "Google sign-in";
    action_title = "Enter this code";
    action_value = text_present(auth_user_code) ? auth_user_code : "Waiting for Google code";
    action_hint = text_present(auth_verification_uri)
                      ? auth_verification_uri
                      : "Waiting for the Google sign-in address";
    next_value = "Approve Google access and the frame will finish automatically.";
    next_hint = "Keep this screen visible so you can copy the code exactly as shown.";
  }
  else if (step == 2)
  {
    badge = "Connect to the frame";
    action_title = "Setup network";
    action_value = text_present(provisioning_ssid) ? provisioning_ssid : "Finding setup network";
    action_hint = text_present(provisioning_password)
                      ? provisioning_password
                      : "This setup network is open. No password is required.";
    next_value = "After joining the setup network, continue in your browser or wait for local setup to appear.";
    next_hint = "If a browser page is available, it will appear below as a direct shortcut.";
  }
  else if (step == 0)
  {
    badge = "Starting up";
  }

  if (text_present(local_setup_url))
  {
    access_title = pairing_browser ? "Browser address" : "Browser setup";
    access_value = local_setup_url;
    access_hint = pairing_browser
                      ? (text_present(local_setup_ip_url)
                             ? local_setup_ip_url
                             : "Open this address, then enter the pass code shown above.")
                      : (text_present(local_setup_ip_url)
                             ? local_setup_ip_url
                             : (text_present(pairing_code) ? pairing_code : "Use this shortcut when setting up from another device."));
  }
  else if (text_present(auth_verification_uri))
  {
    access_value = auth_verification_uri;
    access_hint = text_present(local_setup_ip_url)
                      ? local_setup_ip_url
                      : (text_present(pairing_code) ? pairing_code : access_hint);
  }
  else if (text_present(local_setup_ip_url))
  {
    access_value = local_setup_ip_url;
  }

  lv_label_set_text(s_phase_badge, badge);
  lv_label_set_text(s_headline, text_present(headline) ? headline : "Welcome home");
  lv_label_set_text(s_status, text_present(status) ? status : "Booting up the display and preparing setup.");
  lv_label_set_text(s_detail, text_present(detail) ? detail : "The frame will guide you through setup as soon as it is ready.");
  lv_label_set_text(s_connection_value, text_present(network) ? network : "Unknown");
  lv_label_set_text(s_connection_hint, text_present(detail) ? detail : "Current network details will appear here.");
  lv_label_set_text(s_action_title, action_title);
  lv_label_set_text(s_action_value, action_value);
  lv_label_set_text(s_action_hint, action_hint);

  set_tile(0, "What happens next", next_value, next_hint);
  set_tile(1, access_title, access_value, access_hint);
  set_tile(2, "Account", account_value, account_hint);
  set_tile(3,
           "State summary",
           text_present(headline) ? headline : "Welcome home",
           text_present(detail) ? detail : "Preparing guided setup.");

  for (size_t i = 0; i < 4; ++i)
  {
    bool active = (int)(i + 1) == step || (i == 0 && step == 0);
    bool complete = (int)(i + 1) < step;
    if (step == 4 && i == 3)
    {
      active = true;
      complete = true;
    }
    set_step_style(i, active, complete);
  }

  unlock_display();
  return ESP_OK;
}