#pragma once

#include "esp_err.h"

#ifdef __cplusplus
extern "C"
{
#endif

  esp_err_t frame_embedded_ui_start(void);
  esp_err_t frame_embedded_panel_init(uint16_t width,
                                      uint16_t height,
                                      uint16_t rotation_degrees);
  esp_err_t frame_embedded_panel_present(const uint16_t *pixels,
                                         uint16_t width,
                                         uint16_t height);
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
                                   const char *auth_verification_uri);

#ifdef __cplusplus
}
#endif