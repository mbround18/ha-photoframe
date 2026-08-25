/*
 * SPDX-FileCopyrightText: 2024 Espressif Systems (Shanghai) CO LTD
 *
 * SPDX-License-Identifier: Apache-2.0
 */

#pragma once

/**************************************************************************************************
 * BSP configuration
 **************************************************************************************************/
// By default, this BSP is shipped with LVGL graphical library. Enabling this option will exclude it.
// If you want to use BSP without LVGL, select BSP version with 'noglib' suffix.
#if !defined(BSP_CONFIG_NO_GRAPHIC_LIB) // Check if the symbol is not coming from compiler definitions (-D...)
/* This firmware renders with Slint and does not use LVGL, so the BSP's
   graphics half is compiled out along with its esp_lvgl_port / lvgl
   dependencies. See idf_component.yml. */
#define BSP_CONFIG_NO_GRAPHIC_LIB (1)
#endif
