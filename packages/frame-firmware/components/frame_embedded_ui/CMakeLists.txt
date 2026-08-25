idf_component_register(
    SRCS "frame_embedded_ui.c"
    INCLUDE_DIRS "include"
    REQUIRES esp32_p4_function_ev_board esp_lcd esp_driver_ledc
             espressif__esp_lcd_jd9365
)
