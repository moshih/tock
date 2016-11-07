use common::Queue;
use platform::{Chip, Platform, MPU, SysTick};
use process;
use process::Process;
use syscall;

pub unsafe fn do_process<P: Platform, C: Chip>(platform: &mut P,
                                               chip: &mut C,
                                               process: &mut Process,
                                               appid: ::AppId) {
    let systick = chip.systick();
    systick.reset();
    systick.set_timer(10000);
    systick.enable(true);

    loop {
        if chip.has_pending_interrupts() || systick.overflowed() || systick.value() <= 500 {
            break;
        }

        match process.state {
            process::State::Running => {
                let (data_start, data_len, text_start, text_len) = process.memory_regions();
                // Data segment read/write/execute
                chip.mpu().set_mpu(0, data_start as u32, data_len as u32, true, 0b011);
                // Text segment read/execute (no write)
                chip.mpu().set_mpu(1, text_start as u32, text_len as u32, true, 0b111);
                systick.enable(true);
                process.switch_to();
                systick.enable(false);
            }
            process::State::Yielded => {
                match process.callbacks.dequeue() {
                    None => break,
                    Some(cb) => {
                        process.state = process::State::Running;
                        process.push_callback(cb);
                        continue;
                    }
                }
            }
        }

        if !process.syscall_fired() {
            break;
        }

        match process.svc_number() {
            Some(syscall::MEMOP) => {
                let brk_type = process.r0();
                let r1 = process.r1();

                let res = match brk_type {
                    0 /* BRK */ => {
                        process.brk(r1 as *const u8)
                            .map(|_| 0).unwrap_or(-1)
                    },
                    1 /* SBRK */ => {
                        process.sbrk(r1 as isize)
                            .map(|addr| addr as isize).unwrap_or(-1)
                    },
                    _ => -2
                };
                process.set_r0(res);
            }
            Some(syscall::YIELD) => {
                process.state = process::State::Yielded;
                process.pop_syscall_stack();

                // There might be already enqueued callbacks
                continue;
            }
            Some(syscall::SUBSCRIBE) => {
                let driver_num = process.r0();
                let subdriver_num = process.r1();
                let callback_ptr = process.r2() as *mut ();
                let appdata = process.r3();

                let res = platform.with_driver(driver_num, |driver| {
                    let callback = ::Callback::new(appid, appdata, callback_ptr);
                    match driver {
                        Some(d) => d.subscribe(subdriver_num, callback),
                        None => -1,
                    }
                });
                process.set_r0(res);
            }
            Some(syscall::COMMAND) => {
                let res = platform.with_driver(process.r0(), |driver| {
                    match driver {
                        Some(d) => d.command(process.r1(), process.r2(), appid),
                        None => -1,
                    }
                });
                process.set_r0(res);
            }
            Some(syscall::ALLOW) => {
                let res = platform.with_driver(process.r0(), |driver| {
                    match driver {
                        Some(d) => {
                            let start_addr = process.r2() as *mut u8;
                            let size = process.r3();
                            if process.in_exposed_bounds(start_addr, size) {
                                let slice = ::AppSlice::new(start_addr as *mut u8, size, appid);
                                d.allow(appid, process.r1(), slice)
                            } else {
                                -1
                            }
                        }
                        None => -1,
                    }
                });
                process.set_r0(res);
            }
            _ => {}
        }
    }
    systick.reset();
}
