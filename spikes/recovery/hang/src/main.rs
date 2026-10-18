//! Stands in for oh-my-posh.exe on a test VM (plan 3b-1 Task 12): it never answers, so a cmd
//! window whose init.lua runs `oh-my-posh init cmd` never reaches its prompt, as a cmd that
//! fails to start. Test VMs only: never started on the dev PC.
fn main() {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
