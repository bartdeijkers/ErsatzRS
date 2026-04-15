mod ffmpeg;

fn main() {
    match ffmpeg::check_ffmpeg_version() {
        Ok(version) => {
            println!("FFmpeg 8.1+ detected (found: {version})");
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }

    println!("nastv-web starting...");
}
