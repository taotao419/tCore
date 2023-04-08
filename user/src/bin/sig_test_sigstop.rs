#![no_std]
#![no_main]

extern crate user_lib;
use user_lib::*;

#[no_mangle]
pub fn main() -> i32 {
    // parent process ------------------SIGSTOP--------------------SIGCONT-----------
    //                  |                 |                          |
    //                  |                 V                          V
    // child process    ------------------*********frozen*************--------------

    let pid = fork();
    if pid == 0 {
        //child process
        sleep(1000);
        println!("signal SIGSTOP Testing: child done");
        exit(0);
    } else if pid > 0 {
        //parent process
        println!("singal SIGSTOP Testing: parent send SIGSTOP to child");
        sleep(500);
        if kill(pid as usize, SIGSTOP) < 0 {
            println!("Send STOP signal failed!");
            exit(1);
        }
        //wait 2000 ms, then send signal continue.
        sleep(2000);
        if kill(pid as usize, SIGCONT) < 0 {
            println!("Send CONTINUE signal failed!");
            exit(1);
        }
        println!("signal SIGSTOP Testing: parent wait child");
        let mut exit_code = 0;
        waitpid(pid as usize, &mut exit_code);
        println!("signal SIGSTOP Testing: parent Done");
        exit(0);
    }
    return 0;
}
