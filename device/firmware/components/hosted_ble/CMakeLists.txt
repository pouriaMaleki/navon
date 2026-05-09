idf_component_register(
    SRCS "hosted_ble.c" "hosted_ble_route_sync.c"
    INCLUDE_DIRS "include"
    PRIV_REQUIRES bt esp_hosted log)
