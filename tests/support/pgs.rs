//! Synthetic PGS rectangle packets shared by actual subtitle playback checks.

/// Emit independent epochs containing a 60x24 white rectangle at `origin`.
pub fn rectangles(
    canvas: [u16; 2],
    origin: [u16; 2],
    seconds: impl Iterator<Item = u16>,
) -> Vec<u8> {
    let mut output = Vec::new();
    for second in seconds {
        let mut presentation = Vec::new();
        presentation.extend_from_slice(&canvas[0].to_be_bytes());
        presentation.extend_from_slice(&canvas[1].to_be_bytes());
        presentation.push(0x30);
        presentation.extend_from_slice(&second.to_be_bytes());
        presentation.extend_from_slice(&[0x80, 0, 0, 1, 0, 0, 0, 0]);
        presentation.extend_from_slice(&origin[0].to_be_bytes());
        presentation.extend_from_slice(&origin[1].to_be_bytes());
        segment(&mut output, second, 0x16, &presentation);
        let mut window = vec![1, 0];
        for value in [origin[0], origin[1], 60, 24] {
            window.extend_from_slice(&value.to_be_bytes());
        }
        segment(&mut output, second, 0x17, &window);
        segment(
            &mut output,
            second,
            0x14,
            &[0, 0, 0, 16, 128, 128, 0, 1, 235, 128, 128, 255],
        );
        let mut object = vec![0, 0, 0, 0xc0, 0, 0, 124, 0, 60, 0, 24];
        for _ in 0..24 {
            object.extend_from_slice(&[0, 0xbc, 1, 0, 0]);
        }
        segment(&mut output, second, 0x15, &object);
        segment(&mut output, second, 0x80, &[]);
    }
    output
}

fn segment(output: &mut Vec<u8>, second: u16, kind: u8, payload: &[u8]) {
    output.extend_from_slice(b"PG");
    for _ in 0..2 {
        output.extend_from_slice(&(u32::from(second) * 90_000).to_be_bytes());
    }
    output.push(kind);
    output.extend_from_slice(&u16::try_from(payload.len()).unwrap().to_be_bytes());
    output.extend_from_slice(payload);
}
