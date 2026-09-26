//! Procedurally drawn application icon (a keycap with a lightning bolt).
//!
//! Drawn in code so that the artwork is original to this project and needs no
//! image files. Also used by `build.rs` to produce the executable's icon.

/// Visual variant of the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconVariant {
    Active,
    Paused,
    /// A macro is running.
    Running,
}

type Rgb = (f32, f32, f32);

const BODY_ACTIVE: (Rgb, Rgb) = ((0.20, 0.47, 0.96), (0.09, 0.27, 0.72));
const FACE_ACTIVE: (Rgb, Rgb) = ((0.36, 0.60, 1.00), (0.18, 0.43, 0.93));
const BODY_PAUSED: (Rgb, Rgb) = ((0.52, 0.55, 0.60), (0.34, 0.36, 0.40));
const FACE_PAUSED: (Rgb, Rgb) = ((0.68, 0.70, 0.74), (0.52, 0.55, 0.60));

/// Lightning bolt, in unit coordinates of the key face.
const PLAY: [(f32, f32); 3] = [(0.34, 0.20), (0.34, 0.80), (0.76, 0.50)];

const BOLT: [(f32, f32); 7] = [(0.56, 0.10), (0.27, 0.56), (0.46, 0.56), (0.38, 0.92), (0.74, 0.40), (0.55, 0.40), (0.70, 0.10)];

fn rounded_rect_contains(x: f32, y: f32, left: f32, top: f32, right: f32, bottom: f32, radius: f32) -> bool {
    if x < left || x > right || y < top || y > bottom {
        return false;
    }
    let cx = x.clamp(left + radius, right - radius);
    let cy = y.clamp(top + radius, bottom - radius);
    let (dx, dy) = (x - cx, y - cy);
    dx * dx + dy * dy <= radius * radius
}

fn polygon_contains(x: f32, y: f32, points: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

/// Renders the icon as straight RGBA pixels (row-major, top row first).
pub fn render(size: u32, variant: IconVariant) -> Vec<u8> {
    let s = size as f32;
    let margin = (s * 0.06).max(0.5);
    let (left, top, right, bottom) = (margin, margin, s - margin, s - margin);
    let radius = s * 0.22;
    // Key face: inset, leaving a darker "edge" at the bottom.
    let inset = (s * 0.07).max(1.0);
    let (fl, ft, fr, fb) = (left + inset * 0.6, top + inset * 0.4, right - inset * 0.6, bottom - inset * 1.6);
    let face_radius = radius * 0.8;
    let (body, face) = match variant {
        IconVariant::Active | IconVariant::Running => (BODY_ACTIVE, FACE_ACTIVE),
        IconVariant::Paused => (BODY_PAUSED, FACE_PAUSED),
    };
    const SAMPLES: u32 = 4;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for py in 0..size {
        for px in 0..size {
            let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let x = px as f32 + (sx as f32 + 0.5) / SAMPLES as f32;
                    let y = py as f32 + (sy as f32 + 0.5) / SAMPLES as f32;
                    if !rounded_rect_contains(x, y, left, top, right, bottom, radius) {
                        continue;
                    }
                    let t = ((y - top) / (bottom - top)).clamp(0.0, 1.0);
                    let mut color = lerp(body.0, body.1, t);
                    if rounded_rect_contains(x, y, fl, ft, fr, fb, face_radius) {
                        let ft_t = ((y - ft) / (fb - ft)).clamp(0.0, 1.0);
                        color = lerp(face.0, face.1, ft_t);
                        let u = (x - fl) / (fr - fl);
                        let v = (y - ft) / (fb - ft);
                        let glyph = match variant {
                            IconVariant::Active => polygon_contains(u, v, &BOLT),
                            IconVariant::Running => polygon_contains(u, v, &PLAY),
                            IconVariant::Paused => {
                                (0.22..0.70).contains(&v) && ((0.32..0.45).contains(&u) || (0.55..0.68).contains(&u))
                            }
                        };
                        if glyph {
                            color = (1.0, 1.0, 1.0);
                        }
                    }
                    acc.0 += color.0;
                    acc.1 += color.1;
                    acc.2 += color.2;
                    acc.3 += 1.0;
                }
            }
            if acc.3 > 0.0 {
                let n = (SAMPLES * SAMPLES) as f32;
                let i = ((py * size + px) * 4) as usize;
                out[i] = (acc.0 / acc.3 * 255.0).round() as u8;
                out[i + 1] = (acc.1 / acc.3 * 255.0).round() as u8;
                out[i + 2] = (acc.2 / acc.3 * 255.0).round() as u8;
                out[i + 3] = (acc.3 / n * 255.0).round() as u8;
            }
        }
    }
    out
}

/// Encodes several sizes as a Windows `.ico` file (32-bit DIB entries).
#[allow(dead_code)] // used by build.rs
pub fn encode_ico(sizes: &[u32], variant: IconVariant) -> Vec<u8> {
    let mut images = Vec::new();
    for &size in sizes {
        let rgba = render(size, variant);
        let mask_row = size.div_ceil(32) * 4;
        let mut data = Vec::new();
        let header_size = 40u32;
        data.extend_from_slice(&header_size.to_le_bytes());
        data.extend_from_slice(&(size as i32).to_le_bytes());
        data.extend_from_slice(&((size * 2) as i32).to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&32u16.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&(size * size * 4 + mask_row * size).to_le_bytes());
        data.extend_from_slice(&[0u8; 16]);
        for y in (0..size).rev() {
            for x in 0..size {
                let i = ((y * size + x) * 4) as usize;
                data.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
            }
        }
        data.extend(std::iter::repeat_n(0u8, (mask_row * size) as usize));
        images.push((size, data));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, data) in &images {
        let dim = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[dim, dim, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += data.len() as u32;
    }
    for (_, data) in images {
        out.extend_from_slice(&data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_has_opaque_center_and_transparent_corner() {
        let size = 32;
        let px = render(size, IconVariant::Active);
        assert_eq!(px.len(), (size * size * 4) as usize);
        assert_eq!(px[3], 0, "top-left corner is transparent");
        let center = ((16 * size + 16) * 4 + 3) as usize;
        assert_eq!(px[center], 255);
    }

    #[test]
    fn ico_header() {
        let ico = encode_ico(&[16, 32], IconVariant::Paused);
        assert_eq!(&ico[0..6], &[0, 0, 1, 0, 2, 0]);
        assert_eq!(ico[6], 16);
        assert_eq!(ico[22], 32);
    }
}
