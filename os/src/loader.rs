//! Loading user applications into memory

/// Get the total number of applications.
use alloc::vec::Vec;
use lazy_static::*;

pub fn get_num_app() -> usize {
    extern "C" {
        fn _num_app();
    }
    unsafe { (_num_app as usize as *const usize).read_volatile() }
}

/// get applications data
pub fn get_app_data(app_id: usize) -> &'static [u8] {
    extern "C" {
        fn _num_app();
    }
    let num_app_ptr = _num_app as usize as *const usize;
    let num_app = get_num_app();
    let app_start = unsafe { core::slice::from_raw_parts(num_app_ptr.add(1), num_app + 1) };
    assert!(app_id < num_app);
    unsafe {
        core::slice::from_raw_parts(
            app_start[app_id] as *const u8,
            app_start[app_id + 1] - app_start[app_id],
        )
    }
}

lazy_static! {
    //ALL of app's name
    static ref APP_NAMES: Vec<&'static str>={
        let num_app=get_num_app();
        extern "C"{
            fn _app_names();
        }
        //start 作为指针指向_app_names这个地址
        let mut start = _app_names as usize as *const u8;
        let mut v= Vec::new();
        unsafe{
            for _ in 0..num_app{
                let mut end=start;
                //没有结束'\0' end就一直增加1
                while end.read_volatile() != b'\0'{
                    end=end.add(1);
                }
                //切一块内存 从start开始到end结束. 就是这段字符串内存
                let slice= core::slice::from_raw_parts(start, end as usize-start as usize);
                //把内存转成字符串类型
                let str= core::str::from_utf8(slice).unwrap();
                v.push(str);//放入向量
                start=end.add(1);//start 继续向下走 进入下个循环
            }
        }
        return v;
    };
}

#[allow(unused)]
pub fn get_app_data_by_name(name: &str) -> Option<&'static [u8]> {
    let num_app = get_num_app();
    (0..num_app)
        .find(|&i| APP_NAMES[i] == name)
        .map(get_app_data)
}

pub fn list_apps() {
    println!("/**** APPS *****");
    for app in APP_NAMES.iter() {
        println!("{}", app);
    }
    println!("******************/");
}
