use std::mem::DropGuard;
use std::num::ParseIntError;

// Parse all strings and push them into `nums`. On failure `nums` is unchanged
fn parse_all(nums: &mut Vec<u32>, strings: &[&str]) -> Result<(), ParseIntError> {
    let len = nums.len();
    let mut nums = DropGuard::new(nums, move |nums| nums.truncate(len)); // rollback

    for s in strings {
        nums.push(s.parse()?); // on `?` error the guard drops and rolls back
    }

    DropGuard::dismiss(nums); // success: cancel rollback
    Ok(())
}

fn main() {}
