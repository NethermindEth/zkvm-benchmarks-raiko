#![no_main]

#[jolt::provable]
pub fn func() {
    let iterations = 3000 * 1024;
    for i in 0..iterations {
        memory_barrier(&i);
    }
}

#[allow(unused_variables)]
pub fn memory_barrier<T>(ptr: *const T) {
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst)
}
