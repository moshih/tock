#ifndef _TMP006_H
#define _TMP006_H

#include <tock.h>

#ifdef __cplusplus
extern "C" {
#endif

#define ERR_NONE 0

int tmp006_read_sync(int16_t* temp_reading);
int tmp006_read_async(subscribe_cb callback, void* callback_args);
int tmp006_start_sampling(uint8_t period, subscribe_cb callback, void* callback_args);
int tmp006_stop_sampling();

#ifdef __cplusplus
}
#endif

#endif // _TMP006_H
