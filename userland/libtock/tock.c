#include <inttypes.h>
#include <stdlib.h>
#include <unistd.h>
#include "tock.h"

#pragma GCC diagnostic ignored "-Wunused-parameter"

void yield_for(bool *cond) {
  while(!*cond) {
    yield();
  }
}

void __attribute__((naked)) yield() {
  asm volatile("push {lr}\nsvc 0\npop {pc}" ::: "memory", "r0");
}

int __attribute__((naked)) subscribe(uint32_t driver, uint32_t subscribe,
              subscribe_cb cb, void* userdata) {
  asm volatile("svc 1\nbx lr" ::: "memory", "r0");
}


int __attribute__((naked))
command(uint32_t driver, uint32_t command, int data) {
  asm volatile("svc 2\nbx lr" ::: "memory", "r0");
}

int __attribute__((naked))
allow(uint32_t driver, uint32_t allow, void* ptr, size_t size) {
  asm volatile("svc 3\nbx lr" ::: "memory", "r0");
}

int __attribute__((naked)) memop(uint32_t op_type, int arg1) {
  asm volatile("svc 4\nbx lr" ::: "memory", "r0");
}

