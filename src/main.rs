use libc::c_void;
use nix::sys::mman::{mmap, msync, MapFlags, MsFlags, ProtFlags};
use std::env;
use std::fs::File;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::{thread, time};

const MB: usize = 1024 * 1024;
const GB: usize = 1024 * MB;

const TO_WRITE: usize = 8 * GB;

const AMP: u64 = 0x2626262626262626;
const DOLLAR: u64 = 0x2424242424242424;
const PERC: u64 = 0x2525252525252525;
const HASH: u64 = 0x2323232323232323;

/// Read an entire region pointed at by ptr and of size region_size
/// returns how long it took to read it in milliseconds
fn read_and_measure(ptr: &*mut u64, region_size: usize) -> u128 {
    let begin = time::Instant::now();
    unsafe {
        println!("{:?}", ptr);
        let mut s = 0;
        for i in 0..(region_size / 8) {
            s += *ptr.offset(i as isize);
        }
        println!("{}", s); // need to do something with the read data, otherwise the compiler might optimize it out
    }
    let after = begin.elapsed();
    after.as_millis()
}

/// Write an entire region pointed at by ptr and of size region_size
/// returns how long it took to write it in milliseconds
fn write_and_measure(
    ptr: &*mut u64,
    region_size: usize,
    to_disk: bool,
    wait_to_flush: bool,
    chars_to_write: u64,
) -> u128 {
    let begin = time::Instant::now();
    unsafe {
        println!("{:?}", ptr);
        for i in 0..(region_size / 8) {
            *ptr.offset(i as isize) = chars_to_write; // fill the region
        }
    }
    if to_disk == true {
        // sync to disk
        unsafe {
            msync(*ptr as *mut c_void, TO_WRITE, MsFlags::MS_SYNC).expect("msync failed");
        }
        if wait_to_flush == true {
            // reclaim the pages from memory
            unsafe {
                // MADV_PAGEOUT is 21, see: sysdeps/unix/sysv/linux/bits/mman-linux.h
                // unfortunately, the rust bindings for libc do not have MADV_PAGEOUT
                libc::madvise(*ptr as *mut c_void, TO_WRITE, 21);
            }
        }
    }
    let after = begin.elapsed();
    after.as_millis()
}

unsafe fn get_pointer_to_region_backing_fd(fd: i32) -> *mut c_void {
    // mmap a region backed by a file
    let addr = mmap(
        std::ptr::null_mut(),
        TO_WRITE,
        ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
        MapFlags::MAP_SHARED,
        fd,
        0,
    )
    .expect("mmap failed");
    addr
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let filepath = format!("{}", args[1]);
    let path = Path::new(&filepath);

    println!("measuring single threaded memory and disk bandwidth, stay tuned!");

    // create the backing file if it doesn't exist
    let file = match File::options()
        .create(true)
        .read(true)
        .write(true)
        .open(&path)
    {
        Err(_why) => panic!("couldn't open file"),
        Ok(file) => file,
    };
    let fd = file.as_raw_fd();
    // need to make the file the right size for memmapping
    unsafe {
        libc::ftruncate(fd, TO_WRITE as i64);
    }

    // get pointer to the region backed by the file
    let addr = unsafe { get_pointer_to_region_backing_fd(fd) };
    let ptr = (addr as usize) as *mut u64;
    //let ptr = unsafe { libc::malloc(TO_WRITE) as *mut u64 };

    let write_unmapped = write_and_measure(&ptr, TO_WRITE, false, false, AMP);
    let write_mapped = write_and_measure(&ptr, TO_WRITE, false, false, DOLLAR);

    let read_mapped = read_and_measure(&ptr, TO_WRITE);

    let write_and_sync = write_and_measure(&ptr, TO_WRITE, true, false, HASH);
    let write_and_sync_flush = write_and_measure(&ptr, TO_WRITE, true, true, PERC);

    let read_from_disk = read_and_measure(&ptr, TO_WRITE);

    // unmap, but the file will still be in the page cache
    unsafe { libc::munmap(addr, TO_WRITE) };
    // map again so that pages are in the page cache, but unmapped
    // get pointer to the region backed by the file
    let addr = unsafe { get_pointer_to_region_backing_fd(fd) };
    let ptr = (addr as usize) as *mut u64;

    // read unmapped
    let read_unmapped = read_and_measure(&ptr, TO_WRITE);

    println!(
        "write_unmapped took {}ms for a bw of {} MB/s\n \
        write_mapped took {}ms for a bw of {} MB/s\n \
        write_mapped_and_sync took {}ms for a bw of {} MB/s\n \
        write_mapped_and_sync_flush took {}ms for a bw of {} MB/s\n \
        read_mapped took {}ms for a bw of {} MB/s\n \
        read_from_disk took {}ms for a bw of {} MB/s\n \
        read_unmapped took {}ms for a bw of {} MB/s\n",
        write_unmapped,
        TO_WRITE / (write_unmapped as usize * 1000),
        write_mapped,
        TO_WRITE / (write_mapped as usize * 1000),
        write_and_sync,
        TO_WRITE / (write_and_sync as usize * 1000),
        write_and_sync_flush,
        TO_WRITE / (write_and_sync_flush as usize * 1000),
        read_mapped,
        TO_WRITE / (read_mapped as usize * 1000),
        read_from_disk,
        TO_WRITE / (read_from_disk as usize * 1000),
        read_unmapped,
        TO_WRITE / (read_unmapped as usize * 1000),
    );

    unsafe {
        libc::munmap(addr, TO_WRITE);
        libc::close(fd);
    }

    // thread::sleep(time::Duration::from_secs(1000));
}
