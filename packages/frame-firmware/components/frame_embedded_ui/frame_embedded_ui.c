#include <stdio.h>
#include <stdbool.h>
#include <stdint.h>

#include "esp_check.h"
#include "esp_err.h"
#include "esp_log.h"
#include "frame_embedded_ui.h"
#include "bsp/esp32_p4_function_ev_board.h"

static const char *TAG = "frame_embedded_ui";
static bool s_started;
static lv_obj_t *s_headline;
static lv_obj_t *s_status;
static lv_obj_t *s_network;
static lv_obj_t *s_detail;

static esp_err_t lock_display(void)
{
  return bsp_display_lock(1000) ? ESP_OK : ESP_ERR_TIMEOUT;
}

static void unlock_display(void)
{
  bsp_display_unlock();
}

static esp_err_t enable_vendor_backlight(void)
{
  return bsp_display_backlight_on();
}

esp_err_t frame_embedded_ui_start(void)
{
  if (s_started)
  {
    return ESP_OK;
  }

  lv_display_t *display = bsp_display_start();
  ESP_RETURN_ON_FALSE(display != NULL, ESP_FAIL, TAG, "display register failed");
  ESP_RETURN_ON_ERROR(enable_vendor_backlight(), TAG, "vendor backlight enable failed");
  ESP_RETURN_ON_ERROR(lock_display(), TAG, "display lock failed");

  lv_obj_t *screen = lv_screen_active();
  lv_obj_set_style_bg_color(screen, lv_color_hex(0x0B1220), 0);
  lv_obj_set_style_bg_opa(screen, LV_OPA_COVER, 0);

  s_headline = lv_label_create(screen);
  lv_obj_set_width(s_headline, 680);
  lv_obj_set_style_text_align(s_headline, LV_TEXT_ALIGN_CENTER, 0);
  lv_obj_set_style_text_color(s_headline, lv_color_hex(0xF8FAFC), 0);
  lv_obj_align(s_headline, LV_ALIGN_TOP_MID, 0, 240);

  s_status = lv_label_create(screen);
  lv_obj_set_width(s_status, 700);
  lv_label_set_long_mode(s_status, LV_LABEL_LONG_WRAP);
  lv_obj_set_style_text_align(s_status, LV_TEXT_ALIGN_CENTER, 0);
  lv_obj_set_style_text_color(s_status, lv_color_hex(0xDBE4F0), 0);
  lv_obj_align(s_status, LV_ALIGN_TOP_MID, 0, 340);

  s_network = lv_label_create(screen);
  lv_obj_set_width(s_network, 700);
  lv_obj_set_style_text_align(s_network, LV_TEXT_ALIGN_CENTER, 0);
  lv_obj_set_style_text_color(s_network, lv_color_hex(0x7DD3FC), 0);
  lv_obj_align(s_network, LV_ALIGN_TOP_MID, 0, 430);

  s_detail = lv_label_create(screen);
  lv_obj_set_width(s_detail, 720);
  lv_label_set_long_mode(s_detail, LV_LABEL_LONG_WRAP);
  lv_obj_set_style_text_align(s_detail, LV_TEXT_ALIGN_CENTER, 0);
  lv_obj_set_style_text_color(s_detail, lv_color_hex(0x94A3B8), 0);
  lv_obj_align(s_detail, LV_ALIGN_TOP_MID, 0, 510);

  lv_label_set_text(s_headline, "Welcome");
  lv_label_set_text(s_status, "Starting the photo frame");
  lv_label_set_text(s_network, "Network status: Unprovisioned");
  lv_label_set_text(s_detail, "Preparing the setup flow.");

  unlock_display();
  s_started = true;
  ESP_LOGI(TAG, "embedded display started");

  return ESP_OK;
}

esp_err_t frame_embedded_ui_sync(const char *headline,
                                 const char *status,
                                 const char *network,
                                 const char *detail)
{
  ESP_RETURN_ON_FALSE(s_started, ESP_ERR_INVALID_STATE, TAG, "display not started");
  ESP_RETURN_ON_ERROR(lock_display(), TAG, "display lock failed");

  lv_label_set_text(s_headline, headline != NULL ? headline : "Welcome");
  lv_label_set_text(s_status, status != NULL ? status : "Starting the photo frame");

  static char network_line[96];
  snprintf(network_line,
           sizeof(network_line),
           "Network status: %s",
           network != NULL ? network : "Unknown");
  lv_label_set_text(s_network, network_line);
  lv_label_set_text(s_detail, detail != NULL ? detail : "");

  unlock_display();
  return ESP_OK;
}