use image::{Rgb, RgbImage};

fn s15(value: f64) -> [u8; 4] {
    ((value * 65536.0).round() as i32).to_be_bytes()
}

fn srgb_curve(count: usize) -> Vec<u8> {
    let mut tag = b"curv\0\0\0\0".to_vec();
    tag.extend_from_slice(&(count as u32).to_be_bytes());
    for at in 0..count {
        let v = at as f64 / (count - 1) as f64;
        let linear = if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        };
        tag.extend_from_slice(&((linear * 65535.0).round() as u16).to_be_bytes());
    }
    tag
}

fn xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut tag = b"XYZ \0\0\0\0".to_vec();
    for value in [x, y, z] {
        tag.extend_from_slice(&s15(value));
    }
    tag
}

pub fn p3_profile() -> Vec<u8> {
    let curve = srgb_curve(1024);
    let mut text = b"text\0\0\0\0".to_vec();
    text.extend_from_slice(b"public domain\0");
    let mut desc = b"desc\0\0\0\0".to_vec();
    desc.extend_from_slice(&11u32.to_be_bytes());
    desc.extend_from_slice(b"Display P3\0");
    desc.extend_from_slice(&[0; 4 + 4 + 2 + 1 + 67]);
    let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"desc", desc),
        (b"cprt", text),
        (b"wtpt", xyz_tag(0.9642, 1.0, 0.8249)),
        (b"rXYZ", xyz_tag(0.5151, 0.2412, -0.0011)),
        (b"gXYZ", xyz_tag(0.2920, 0.6922, 0.0419)),
        (b"bXYZ", xyz_tag(0.1571, 0.0666, 0.7841)),
        (b"rTRC", curve.clone()),
        (b"gTRC", curve.clone()),
        (b"bTRC", curve),
    ];
    let table = 4 + tags.len() * 12;
    let mut offset = 128 + table;
    let mut directory = (tags.len() as u32).to_be_bytes().to_vec();
    let mut body = Vec::new();
    for (signature, data) in &tags {
        let padded = data.len().div_ceil(4) * 4;
        directory.extend_from_slice(*signature);
        directory.extend_from_slice(&(offset as u32).to_be_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
        body.resize(body.len() + padded - data.len(), 0);
        offset += padded;
    }
    let mut profile = vec![0u8; 128];
    profile[8..12].copy_from_slice(&[2, 0x40, 0, 0]);
    profile[12..16].copy_from_slice(b"mntr");
    profile[16..20].copy_from_slice(b"RGB ");
    profile[20..24].copy_from_slice(b"XYZ ");
    profile[36..40].copy_from_slice(b"acsp");
    for (at, value) in [0.9642, 1.0, 0.8249].into_iter().enumerate() {
        profile[68 + at * 4..72 + at * 4].copy_from_slice(&s15(value));
    }
    profile.extend(directory);
    profile.extend(body);
    let size = profile.len() as u32;
    profile[..4].copy_from_slice(&size.to_be_bytes());
    profile
}

pub fn p3_to_srgb(rgb: [u8; 3]) -> [u8; 3] {
    const MATRIX: [[f64; 3]; 3] = [
        [1.2249, -0.2247, 0.0],
        [-0.0420, 1.0419, 0.0],
        [-0.0197, -0.0786, 1.0979],
    ];
    let decode = |v: u8| {
        let v = v as f64 / 255.0;
        match v <= 0.04045 {
            true => v / 12.92,
            false => ((v + 0.055) / 1.055).powf(2.4),
        }
    };
    let encode = |l: f64| {
        let l = l.clamp(0.0, 1.0);
        let v = match l <= 0.0031308 {
            true => l * 12.92,
            false => 1.055 * l.powf(1.0 / 2.4) - 0.055,
        };
        (v * 255.0).round() as u8
    };
    let linear = rgb.map(decode);
    MATRIX.map(|row| encode(row.iter().zip(linear).map(|(m, c)| m * c).sum()))
}

pub fn flat(w: u32, h: u32, rgb: [u8; 3]) -> RgbImage {
    RgbImage::from_pixel(w, h, Rgb(rgb))
}

pub fn with_icc_jpeg(jpeg: &[u8], profile: &[u8]) -> Vec<u8> {
    let mut segment = b"ICC_PROFILE\0\x01\x01".to_vec();
    segment.extend_from_slice(profile);
    let mut out = jpeg[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE2]);
    out.extend_from_slice(&(segment.len() as u16 + 2).to_be_bytes());
    out.extend(segment);
    out.extend_from_slice(&jpeg[2..]);
    out
}

pub fn with_icc_png(png: &[u8], profile: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    zlib.write_all(profile).unwrap();
    let mut data = b"p3\0\0".to_vec();
    data.extend(zlib.finish().unwrap());
    let mut body = b"iCCP".to_vec();
    body.extend(data.iter());
    let mut chunk = (data.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(&body);
    chunk.extend_from_slice(&crc32fast::hash(&body).to_be_bytes());
    let at = 8 + 8 + 13 + 4;
    let mut out = png[..at].to_vec();
    out.extend(chunk);
    out.extend_from_slice(&png[at..]);
    out
}

pub fn with_icc_webp(webp: &[u8], profile: &[u8], w: u32, h: u32) -> Vec<u8> {
    let chunk = |name: &[u8; 4], data: &[u8]| {
        let mut out = name.to_vec();
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(data);
        if data.len() % 2 == 1 {
            out.push(0);
        }
        out
    };
    let mut canvas = vec![0x20, 0, 0, 0];
    canvas.extend_from_slice(&(w - 1).to_le_bytes()[..3]);
    canvas.extend_from_slice(&(h - 1).to_le_bytes()[..3]);
    let mut body = b"WEBP".to_vec();
    body.extend(chunk(b"VP8X", &canvas));
    body.extend(chunk(b"ICCP", profile));
    body.extend_from_slice(&webp[12..]);
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}
