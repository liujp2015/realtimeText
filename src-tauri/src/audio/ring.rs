use rtrb::{Consumer, Producer, RingBuffer};

const CAPACITY_FRAMES: usize = 192_000;

pub fn new() -> (Producer<f32>, Consumer<f32>) {
    RingBuffer::<f32>::new(CAPACITY_FRAMES)
}
