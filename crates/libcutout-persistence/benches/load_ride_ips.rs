//! Wall-clock throughput for the durable ride-detail load path.

use std::hint::black_box;

use divan::Bencher;

#[path = "support/load_ride.rs"]
mod load_ride_support;
use load_ride_support::Fixture;

#[divan::bench]
fn load_ride(bencher: Bencher<'_, '_>) {
    let fixture = Fixture::setup();
    bencher.bench(|| black_box(fixture.load()));
    fixture.assert_matches(&fixture.load());
    fixture.shutdown();
}

fn main() {
    divan::main();
}
