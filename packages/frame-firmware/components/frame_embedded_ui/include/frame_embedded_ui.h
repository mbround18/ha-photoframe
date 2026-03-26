#pragma once

#include "esp_err.h"

#ifdef __cplusplus
extern "C"
{
#endif

  esp_err_t frame_embedded_ui_start(void);
  esp_err_t frame_embedded_ui_sync(const char *headline,
                                   const char *status,
                                   const char *network,
                                   const char *detail);

#ifdef __cplusplus
}
#endif