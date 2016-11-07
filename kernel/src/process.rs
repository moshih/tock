use callback::AppId;
use common::{RingBuffer, Queue};

use container;
use core::{mem, ptr, slice};
use core::intrinsics::breakpoint;
use core::ptr::{read_volatile, write_volatile};

#[no_mangle]
pub static mut SYSCALL_FIRED: usize = 0;

#[allow(improper_ctypes)]
extern "C" {
    pub fn switch_to_user(user_stack: *const u8, mem_base: *const u8) -> *mut u8;
}

pub static mut PROCS: &'static mut [Option<Process<'static>>] = &mut [];

pub fn schedule(callback: Callback, appid: AppId) -> bool {
    let procs = unsafe { &mut PROCS };
    let idx = appid.idx();
    if idx >= procs.len() {
        return false;
    }

    match procs[idx] {
        None => false,
        Some(ref mut p) => {
            // TODO(alevy): validate appid liveness

            p.callbacks.enqueue(callback)
        }
    }
}

#[derive(Copy,Clone,PartialEq,Eq)]
pub enum Error {
    NoSuchApp,
    OutOfMemory,
    AddressOutOfBounds,
}

#[derive(Copy,Clone,PartialEq,Eq)]
pub enum State {
    Running,
    Yielded,
}


#[derive(Copy,Clone,Debug)]
pub struct Callback {
    pub r0: usize,
    pub r1: usize,
    pub r2: usize,
    pub r3: usize,
    pub pc: usize,
}

#[repr(C)]
struct LoadInfo {
    total_size: usize, // Total padded size of the program image
    rel_data_size: usize,
    entry_loc: usize, // Entry point for user application
    init_data_loc: usize, // Data initialization information in flash
    init_data_size: usize, // Size of initialization information
    got_start_offset: usize, // Offset to start of GOT
    got_end_offset: usize, // Offset to end of GOT
    bss_start_offset: usize, // Offset to start of BSS
    bss_end_offset: usize, // Offset to end of BSS
}

pub struct Process<'a> {
    /// The process's memory.
    memory: &'static mut [u8],

    app_memory_break: *const u8,
    kernel_memory_break: *const u8,

    /// Process text segment
    text: &'static [u8],

    /// The offset in `memory` to use for the process stack.
    cur_stack: *const u8,

    yield_pc: usize,
    psr: usize,

    pub state: State,

    pub callbacks: RingBuffer<'a, Callback>,
}

impl<'a> Process<'a> {
    pub fn mem_start(&self) -> *const u8 {
        self.memory.as_ptr()
    }

    pub fn mem_end(&self) -> *const u8 {
        unsafe { self.memory.as_ptr().offset(self.memory.len() as isize) }
    }

    pub fn memory_regions(&self) -> (usize, usize, usize, usize) {
        let data_start = self.memory.as_ptr() as usize;
        let data_len = 12;

        let text_start = self.text.as_ptr() as usize;
        let text_len = ((32 - self.text.len().leading_zeros()) - 2) as usize;
        (data_start, data_len, text_start, text_len)
    }

    pub unsafe fn create(start_addr: *const u8,
                         length: usize,
                         memory: &'static mut [u8])
                         -> Process<'a> {
        let mut kernel_memory_break = {
            // make room for container pointers
            let psz = mem::size_of::<*const usize>();
            let num_ctrs = read_volatile(&container::CONTAINER_COUNTER);
            let container_ptrs_size = num_ctrs * psz;
            let res = memory.as_mut_ptr().offset((memory.len() - container_ptrs_size) as isize);
            // set all ptrs to null
            let opts = slice::from_raw_parts_mut(res as *mut *const usize, num_ctrs);
            for opt in opts.iter_mut() {
                *opt = ptr::null()
            }
            res
        };

        // Take callback buffer from of memory
        let callback_size = mem::size_of::<Option<Callback>>();
        let callback_len = 10;
        let callback_offset = callback_len * callback_size;
        // Set kernel break to beginning of callback buffer
        kernel_memory_break = kernel_memory_break.offset(-(callback_offset as isize));
        let callback_buf = slice::from_raw_parts_mut(kernel_memory_break as *mut Callback,
                                                     callback_len);

        let callbacks = RingBuffer::new(callback_buf);

        let load_result = load(start_addr, memory.as_mut_ptr());

        let stack_bottom = load_result.app_mem_start.offset(512);

        let mut process = Process {
            memory: memory,
            app_memory_break: stack_bottom,
            kernel_memory_break: kernel_memory_break,
            text: slice::from_raw_parts(start_addr, length),
            cur_stack: stack_bottom,
            yield_pc: 0,
            psr: 0x01000000,
            state: State::Yielded,
            callbacks: callbacks,
        };

        process.callbacks.enqueue(Callback {
            pc: load_result.init_fn,
            r0: load_result.app_mem_start as usize,
            r1: process.app_memory_break as usize,
            r2: process.kernel_memory_break as usize,
            r3: 0,
        });

        process
    }

    pub fn sbrk(&mut self, increment: isize) -> Result<*const u8, Error> {
        let new_break = unsafe { self.app_memory_break.offset(increment) };
        self.brk(new_break)
    }

    pub fn brk(&mut self, new_break: *const u8) -> Result<*const u8, Error> {
        if new_break < self.mem_start() || new_break >= self.mem_end() {
            Err(Error::AddressOutOfBounds)
        } else if new_break > self.kernel_memory_break {
            Err(Error::OutOfMemory)
        } else {
            let old_break = self.app_memory_break;
            self.app_memory_break = new_break;
            Ok(old_break)
        }
    }

    pub fn in_exposed_bounds(&self, buf_start_addr: *const u8, size: usize) -> bool {

        let buf_end_addr = unsafe { buf_start_addr.offset(size as isize) };

        buf_start_addr >= self.mem_start() && buf_end_addr <= self.mem_end()
    }

    pub unsafe fn alloc(&mut self, size: usize) -> Option<&mut [u8]> {
        let new_break = self.kernel_memory_break.offset(-(size as isize));
        if new_break < self.app_memory_break {
            None
        } else {
            self.kernel_memory_break = new_break;
            Some(slice::from_raw_parts_mut(new_break as *mut u8, size))
        }
    }

    pub unsafe fn free<T>(&mut self, _: *mut T) {}

    pub unsafe fn container_for<T>(&mut self, container_num: usize) -> *mut *mut T {
        let container_num = container_num as isize;
        let ptr = (self.mem_end() as *mut usize).offset(-(container_num + 1));
        ptr as *mut *mut T
    }

    pub unsafe fn container_for_or_alloc<T: Default>(&mut self,
                                                     container_num: usize)
                                                     -> Option<*mut T> {
        let ctr_ptr = self.container_for::<T>(container_num);
        if (*ctr_ptr).is_null() {
            self.alloc(mem::size_of::<T>()).map(|root_arr| {
                let root_ptr = root_arr.as_mut_ptr() as *mut T;
                *root_ptr = Default::default();
                write_volatile(ctr_ptr, root_ptr);
                root_ptr
            })
        } else {
            Some(*ctr_ptr)
        }
    }


    pub fn pop_syscall_stack(&mut self) {
        let pspr = self.cur_stack as *const usize;
        unsafe {
            self.yield_pc = read_volatile(pspr.offset(6));
            self.psr = read_volatile(pspr.offset(7));
            self.cur_stack = (self.cur_stack as *mut usize).offset(8) as *mut u8;
        }
    }

    /// Context switch to the process.
    pub unsafe fn push_callback(&mut self, callback: Callback) {
        // Fill in initial stack expected by SVC handler
        // Top minus 8 u32s for r0-r3, r12, lr, pc and xPSR
        let stack_bottom = (self.cur_stack as *mut usize).offset(-8);
        write_volatile(stack_bottom.offset(7), self.psr);
        write_volatile(stack_bottom.offset(6), callback.pc | 1);
        // Set the LR register to the saved PC so the callback returns to
        // wherever wait was called. Set lowest bit to one because of THUMB
        // instruction requirements.
        write_volatile(stack_bottom.offset(5), self.yield_pc | 0x1);
        write_volatile(stack_bottom, callback.r0);
        write_volatile(stack_bottom.offset(1), callback.r1);
        write_volatile(stack_bottom.offset(2), callback.r2);
        write_volatile(stack_bottom.offset(3), callback.r3);

        self.cur_stack = stack_bottom as *mut u8;
    }

    pub unsafe fn syscall_fired(&self) -> bool {
        read_volatile(&SYSCALL_FIRED) != 0
    }

    /// Context switch to the process.
    pub unsafe fn switch_to(&mut self) {
        if self.cur_stack < self.memory.as_ptr() {
            breakpoint();
        }
        write_volatile(&mut SYSCALL_FIRED, 0);
        let psp = switch_to_user(self.cur_stack, self.memory.as_ptr());
        self.cur_stack = psp;
    }

    pub fn svc_number(&self) -> Option<u8> {
        let psp = self.cur_stack as *const *const u16;
        unsafe {
            let pcptr = read_volatile((psp as *const *const u16).offset(6));
            let svc_instr = read_volatile(pcptr.offset(-1));
            Some((svc_instr & 0xff) as u8)
        }
    }

    pub fn lr(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr.offset(5)) }
    }


    pub fn r0(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr) }
    }

    pub fn set_r0(&mut self, val: isize) {
        let pspr = self.cur_stack as *mut isize;
        unsafe { write_volatile(pspr, val) }
    }

    pub fn r1(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr.offset(1)) }
    }

    pub fn r2(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr.offset(2)) }
    }

    pub fn r3(&self) -> usize {
        let pspr = self.cur_stack as *const usize;
        unsafe { read_volatile(pspr.offset(3)) }
    }
}

struct LoadResult {
    text_start: *const u8,
    text_len: usize,
    init_fn: usize,
    app_mem_start: *const u8,
}

unsafe fn load(start_addr: *const u8, mem_base: *mut u8) -> LoadResult {
    let mut result = LoadResult {
        text_start: start_addr as *const u8,
        text_len: 0,
        init_fn: 0,
        app_mem_start: ptr::null(),
    };

    let load_info = &*(start_addr as *const LoadInfo);

    let rel_data_addr = start_addr.offset(mem::size_of::<LoadInfo>() as isize);
    let rel_data: &[usize] = slice::from_raw_parts(rel_data_addr as *const usize,
                                                   load_info.rel_data_size / 4);

    // Update text location in self
    let text_start = rel_data_addr.offset(load_info.rel_data_size as isize);
    result.text_start = text_start;
    result.text_len = load_info.init_data_loc;

    // Zero out BSS
    ::core::intrinsics::write_bytes(mem_base.offset(load_info.bss_start_offset as isize),
                                    0,
                                    load_info.bss_end_offset - load_info.bss_start_offset);

    // Copy data into Data section
    let init_data: &[u8] =
        slice::from_raw_parts(text_start.offset(load_info.init_data_loc as isize),
                              load_info.init_data_size);

    let target_data: &mut [u8] = slice::from_raw_parts_mut(mem_base, load_info.init_data_size);

    target_data.clone_from_slice(init_data);

    let fixup = |addr: *mut usize| {
        let entry = *addr;
        if (entry & 0x80000000) == 0 {
            // Regular data (memory relative)
            *addr = entry + (mem_base as usize);
        } else {
            // rodata or function pointer (code relative)
            *addr = (entry ^ 0x80000000) + (text_start as usize);
        }
    };

    // Fixup Global Offset Table
    let mut got_cur = mem_base.offset(load_info.got_start_offset as isize) as *mut usize;
    let got_end = mem_base.offset(load_info.got_end_offset as isize) as *mut usize;
    while got_cur != got_end {
        fixup(got_cur);
        got_cur = got_cur.offset(1);
    }

    // Fixup relocation data
    for (i, addr) in rel_data.iter().enumerate() {
        if i % 2 == 0 {
            // Only the first of every 2 entries is an address
            fixup(mem_base.offset(*addr as isize) as *mut usize);
        }
    }

    // Entry point is offset from app code
    result.init_fn = text_start as usize + load_info.entry_loc;

    let mut aligned_mem_start = load_info.bss_end_offset as isize;
    aligned_mem_start += (8 - (aligned_mem_start % 8)) % 8;
    result.app_mem_start = mem_base.offset(aligned_mem_start);

    result
}
