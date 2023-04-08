#![no_std]
#![no_main]

extern crate user_lib;
use user_lib::*;


#[no_mangle]
pub fn main() -> i32 {
    let pid = fork();
    if pid == 0 {
        //child process
        sleep(1000);
        println!("signal SIGKILL Testing: child done");
        exit(0);
    } else if pid > 0 {
        //parent process
        println!("singal SIGKILL Testing: parent kill child");
        sleep(500);
        if kill(pid as usize, SIGKILL) < 0 {
            println!("Send signal failed!");
            exit(1);
        }
        println!("signal SIGKILL Testing: parent wait child");
        let mut exit_code = 0;
        waitpid(pid as usize, &mut exit_code);
        println!("signal SIGKILL Testing: parent Done");
        exit(0);
    }
    return 0;
}
