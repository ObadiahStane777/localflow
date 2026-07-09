//! Headless STT verification harness (no mic/hotkey/permissions needed):
//!   cargo run --example transcribe_file -- whisper-cpp <model.bin> <audio.wav>
//!   cargo run --example transcribe_file -- parakeet <model-dir> <audio.wav>

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().init();
    let mut args = std::env::args().skip(1);
    let usage = "usage: transcribe_file <whisper-cpp|parakeet> <model-path> <audio.wav>";
    let engine = args.next().expect(usage);
    let model = args.next().expect(usage);
    let wav = args.next().expect(usage);
    let t0 = std::time::Instant::now();
    let text = localflow::pipeline::transcribe_wav_file(
        &engine,
        std::path::Path::new(&model),
        std::path::Path::new(&wav),
    )?;
    println!("---\n{text}\n--- ({} ms total incl. model load)", t0.elapsed().as_millis());
    Ok(())
}
