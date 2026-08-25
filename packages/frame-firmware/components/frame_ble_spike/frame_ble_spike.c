/*
 * T056 spike: a connectable BLE peripheral on the ESP32-P4.
 *
 * The P4 has no radio. The BLE *controller* runs on the ESP32-C6 and the
 * NimBLE *host* runs here, with HCI carried over the ESP-Hosted SDIO link
 * (CONFIG_BT_CONTROLLER_DISABLED=y + CONFIG_ESP_HOSTED_NIMBLE_HCI_VHCI=y).
 *
 * Modelled on esp_hosted's host_nimble_bleprph_host_only_vhci example, cut
 * down to the minimum that answers the question: can this board advertise a
 * connectable BLE peripheral at all?
 */

#include <string.h>

#include "esp_hosted.h"
#include "esp_log.h"
#include "frame_ble_spike.h"
#include "host/ble_hs.h"
#include "host/util/util.h"
#include "nimble/nimble_port.h"
#include "nimble/nimble_port_freertos.h"
#include "services/gap/ble_svc_gap.h"

static const char *TAG = "frame_ble_spike";

static uint8_t s_own_addr_type;
static char s_device_name[32];

static void start_advertising(void);

static int gap_event(struct ble_gap_event *event, void *arg)
{
  (void)arg;

  switch (event->type)
  {
  case BLE_GAP_EVENT_CONNECT:
    ESP_LOGI(TAG, "BLE connect: status=%d", event->connect.status);
    if (event->connect.status != 0)
    {
      start_advertising();
    }
    break;

  case BLE_GAP_EVENT_DISCONNECT:
    ESP_LOGI(TAG, "BLE disconnect: reason=%d", event->disconnect.reason);
    start_advertising();
    break;

  case BLE_GAP_EVENT_ADV_COMPLETE:
    ESP_LOGI(TAG, "BLE advertising complete; restarting");
    start_advertising();
    break;

  default:
    break;
  }

  return 0;
}

static void start_advertising(void)
{
  struct ble_hs_adv_fields fields;
  memset(&fields, 0, sizeof(fields));

  fields.flags = BLE_HS_ADV_F_DISC_GEN | BLE_HS_ADV_F_BREDR_UNSUP;
  fields.tx_pwr_lvl_is_present = 1;
  fields.tx_pwr_lvl = BLE_HS_ADV_TX_PWR_LVL_AUTO;
  fields.name = (uint8_t *)s_device_name;
  fields.name_len = strlen(s_device_name);
  fields.name_is_complete = 1;

  int rc = ble_gap_adv_set_fields(&fields);
  if (rc != 0)
  {
    ESP_LOGE(TAG, "ble_gap_adv_set_fields failed: %d", rc);
    return;
  }

  struct ble_gap_adv_params adv_params;
  memset(&adv_params, 0, sizeof(adv_params));
  adv_params.conn_mode = BLE_GAP_CONN_MODE_UND;
  adv_params.disc_mode = BLE_GAP_DISC_MODE_GEN;

  rc = ble_gap_adv_start(s_own_addr_type, NULL, BLE_HS_FOREVER, &adv_params,
                         gap_event, NULL);
  if (rc != 0)
  {
    ESP_LOGE(TAG, "ble_gap_adv_start failed: %d", rc);
    return;
  }

  ESP_LOGI(TAG, "advertising as \"%s\"", s_device_name);
}

static void on_sync(void)
{
  int rc = ble_hs_util_ensure_addr(0);
  if (rc != 0)
  {
    ESP_LOGE(TAG, "ble_hs_util_ensure_addr failed: %d", rc);
    return;
  }

  rc = ble_hs_id_infer_auto(0, &s_own_addr_type);
  if (rc != 0)
  {
    ESP_LOGE(TAG, "ble_hs_id_infer_auto failed: %d", rc);
    return;
  }

  uint8_t addr[6] = {0};
  ble_hs_id_copy_addr(s_own_addr_type, addr, NULL);
  ESP_LOGI(TAG, "BLE host synced; address %02x:%02x:%02x:%02x:%02x:%02x",
           addr[5], addr[4], addr[3], addr[2], addr[1], addr[0]);

  start_advertising();
}

static void on_reset(int reason)
{
  ESP_LOGE(TAG, "BLE host reset: reason=%d", reason);
}

static void host_task(void *param)
{
  (void)param;
  ESP_LOGI(TAG, "NimBLE host task started");
  nimble_port_run();
  nimble_port_freertos_deinit();
}

esp_err_t frame_ble_spike_start(const char *device_name)
{
  if (device_name == NULL)
  {
    return ESP_ERR_INVALID_ARG;
  }

  strncpy(s_device_name, device_name, sizeof(s_device_name) - 1);
  s_device_name[sizeof(s_device_name) - 1] = '\0';

  // The controller lives on the C6. Bring it up through ESP-Hosted before the
  // NimBLE host tries to talk HCI to it.
  // These RPCs are new in ESP-Hosted 2.12 and are absent from older slave
  // firmware. Older slaves bring the BT controller up automatically at boot
  // instead, so ESP_ERR_NOT_SUPPORTED is informational, not fatal: keep going
  // and let nimble_port_init() decide whether HCI actually works.
  esp_err_t err = esp_hosted_bt_controller_init();
  if (err == ESP_ERR_NOT_SUPPORTED)
  {
    ESP_LOGW(TAG, "co-processor does not implement the BT-init RPC; "
                  "assuming its controller is already up (older slave firmware)");
  }
  else if (err != ESP_OK)
  {
    ESP_LOGE(TAG, "esp_hosted_bt_controller_init failed: %s", esp_err_to_name(err));
    return err;
  }
  else
  {
    err = esp_hosted_bt_controller_enable();
    if (err != ESP_OK)
    {
      ESP_LOGE(TAG, "esp_hosted_bt_controller_enable failed: %s", esp_err_to_name(err));
      return err;
    }
    ESP_LOGI(TAG, "ESP-Hosted BT controller enabled on the co-processor");
  }

  err = nimble_port_init();
  if (err != ESP_OK)
  {
    ESP_LOGE(TAG, "nimble_port_init failed: %s", esp_err_to_name(err));
    return err;
  }

  ble_hs_cfg.sync_cb = on_sync;
  ble_hs_cfg.reset_cb = on_reset;

  int rc = ble_svc_gap_device_name_set(s_device_name);
  if (rc != 0)
  {
    ESP_LOGE(TAG, "ble_svc_gap_device_name_set failed: %d", rc);
    return ESP_FAIL;
  }

  nimble_port_freertos_init(host_task);
  return ESP_OK;
}
