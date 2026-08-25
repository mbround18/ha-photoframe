/* Exposes the vendored board BSP to Rust through esp-idf-sys' bindgen, via the
 * `bindings_header` entry for that extra_component in Cargo.toml.
 *
 * The BSP is vendored from the manufacturer's own demo bundle rather than taken
 * from the component registry. The registry's espressif/esp32_p4_function_ev_board
 * targets Espressif's reference board: for this panel type it installs an
 * ILI9881C, which this hardware does not answer (it reports ID 0x0/0x0/0x0), and
 * it drives the DSI link at a different lane configuration. It reported success
 * and never lit the screen. See docs/Hardware-Reference.md.
 */
/* esp_cache_msync: the rotated frame buffer lives in PSRAM and is read by DMA,
 * so CPU writes have to be flushed before each transfer. */
#include "esp_cache.h"

#include "bsp/display.h"
#include "bsp/esp32_p4_function_ev_board.h"
