idf_component_register(
    SRCS "sdmap.c"
    INCLUDE_DIRS "include"
    REQUIRES fatfs sdmmc esp_driver_sdmmc vfs heap log)
