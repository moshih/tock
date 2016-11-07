#ifndef _NRF51_SERIALIZATION_H
#define _NRF51_SERIALIZATION_H

#include "tock.h"

#ifdef __cplusplus
extern "C" {
#endif

// Give the BLE Serialization / UART layer a callback to call when
// a packet is received and when a TX is finished.
void nrf51_serialization_subscribe (subscribe_cb cb);

// Pass a buffer for the driver to write received UART bytes to.
void nrf51_serialization_setup_rx_buffer (char* rx, int rx_len);

// Write a packet to the BLE Serialization connectivity processor.
void nrf51_serialization_write (char* tx, int tx_len);

#ifdef __cplusplus
}
#endif

#endif // _NRF51_SERIALIZATION_H
