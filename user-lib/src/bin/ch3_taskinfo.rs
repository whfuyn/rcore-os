#![no_std]
#![no_main]
#![feature(format_args_nl)]

extern crate user_lib;

use user_lib::{
    get_time, println, sleep, task_info, TaskInfo, TaskStatus, SYSCALL_EXIT, SYSCALL_GETTIMEOFDAY,
    SYSCALL_TASK_INFO, SYSCALL_WRITE, SYSCALL_YIELD,
};

#[no_mangle]
pub fn main() -> i32 {
    // println!("get time");
    let t1 = get_time() as usize;
    // println!("get time done");
    let mut info = TaskInfo::new();
    get_time();
    // println!("slepp");
    sleep(500);
    // println!("slepp done");
    let t2 = get_time() as usize;
    // 注意本次 task info 调用也计入
    // println!("task info");
    assert_eq!(0, task_info(&mut info));
    // println!("task info done");
    let t3 = get_time() as usize;
    assert!(3 <= info.syscall_times[SYSCALL_GETTIMEOFDAY]);
    assert_eq!(1, info.syscall_times[SYSCALL_TASK_INFO]);
    assert_eq!(0, info.syscall_times[SYSCALL_WRITE]);
    assert!(0 < info.syscall_times[SYSCALL_YIELD]);
    assert_eq!(0, info.syscall_times[SYSCALL_EXIT]);
    // println!("t1: {t1}");
    // println!("t2: {t2}");
    // println!("info.time: {}", info.time);
    assert!(t2 - t1 <= info.time + 1);
    assert!(info.time < t3 - t1 + 100);
    assert!(info.status == TaskStatus::Running);

    // 想想为什么 write 调用是两次
    println!("string from task info test\n");
    let t4 = get_time() as usize;
    assert_eq!(0, task_info(&mut info));
    let t5 = get_time() as usize;
    assert!(5 <= info.syscall_times[SYSCALL_GETTIMEOFDAY]);
    assert_eq!(2, info.syscall_times[SYSCALL_TASK_INFO]);
    // 我们没有做console的line缓存，所以这里不是两次
    // assert_eq!(2, info.syscall_times[SYSCALL_WRITE]);
    assert!(0 < info.syscall_times[SYSCALL_YIELD]);
    assert_eq!(0, info.syscall_times[SYSCALL_EXIT]);
    assert!(t4 - t1 <= info.time + 1);
    assert!(info.time < t5 - t1 + 100);
    assert!(info.status == TaskStatus::Running);

    println!("Test task info OK!");
    0
}
