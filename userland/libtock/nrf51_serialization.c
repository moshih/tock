#include "nrf51_serialization.h"

void nrf51822_serialization_subscribe (subscribe_cb cb) {
  // get some callback love
  subscribe(5, 0, cb, NULL);
}

void nrf51822_serialization_setup_rx_buffer (char* rx, int rx_len) {
  // Pass the RX buffer for the UART module to use.
  allow(5, 0, rx, rx_len);
}

void nrf51822_serialization_write (char* tx, int tx_len) {
  // Pass in the TX buffer.
  allow(5, 1, tx, tx_len);

  // Do the write!!!!!
  command(5, 0, 0);
}

