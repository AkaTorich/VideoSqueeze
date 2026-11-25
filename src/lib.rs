// VideoSqueeze core: reading files (media), codecs (codec), writing files (mux), one conversion (job).
// The window program (main.rs) and the test file generator (examples/make_tests.rs) use it.
pub mod codec;
pub mod job;
pub mod media;
pub mod mux;
