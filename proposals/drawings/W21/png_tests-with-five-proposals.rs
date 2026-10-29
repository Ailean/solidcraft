use std::cell::RefCell;
use std::f64::consts::{FRAC_PI_2, TAU};

use solidcraft_geom::paper::{Group, Item, NoPictures, ObjectKind, Scene};
use solidcraft_geom::{Aabb2, Loop2, Region2, Seg2, Vec2};

use super::*;

fn p(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}

fn line(a: Vec2, b: Vec2) -> Seg2 {
    Seg2::Line { a, b }
}

/// Anticlockwise from `min`.
fn rectangle(min: Vec2, max: Vec2) -> Loop2 {
    Loop2::polygon(&[min, p(max.x, min.y), max, p(min.x, max.y)])
}

fn scene(size: Vec2, items: Vec<Item>) -> Scene {
    Scene { size_mm: size, groups: vec![Group { object: 1, kind: ObjectKind::DrawingBorder, items }] }
}

/// A 20 × 10 mm sheet: a 1 mm line along y = 5 from x = 2 to 18, and a 6 × 3 mm fill at (2, 1)
/// with a 2 × 1 mm hole at (4, 2). At 254 dpi a millimetre is 10 pixels.
fn small() -> Scene {
    scene(
        p(20.0, 10.0),
        vec![
            Item::Stroke { path: vec![line(p(2.0, 5.0), p(18.0, 5.0))], closed: false, width_mm: 1.0 },
            Item::Fill { region: Region2 { outer: rectangle(p(2.0, 1.0), p(8.0, 4.0)), holes: vec![rectangle(p(4.0, 2.0), p(6.0, 3.0))] } },
        ],
    )
}

pub(in crate::paper) struct Image {
    pub(in crate::paper) info: png::Info<'static>,
    /// One byte a sample, rows top down: 1-bit images unpacked to 0 or 255.
    pub(in crate::paper) samples: Vec<u8>,
}

impl Image {
    fn channels(&self) -> usize {
        self.info.color_type.samples()
    }

    /// The samples of the pixel at column `x`, row `y` from the top.
    pub(in crate::paper) fn at(&self, x: usize, y: usize) -> &[u8] {
        let c = self.channels();
        let i = (y * self.info.width as usize + x) * c;
        &self.samples[i..i + c]
    }
}

fn bytes(scene: &Scene, options: PngOptions) -> (Vec<u8>, PngReport) {
    let mut out = Vec::new();
    let report = paper_png(&scene.prepare().unwrap(), options, &NoPictures, &mut out).unwrap();
    (out, report)
}

pub(in crate::paper) fn read(bytes: &[u8]) -> Image {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes.to_vec()));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().unwrap();
    let mut samples = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut samples).unwrap();
    let info = reader.info().clone();
    if info.bit_depth == png::BitDepth::One {
        // EXPAND makes a 1-bit grey sample 0 or 255 a byte.
        assert!(samples.iter().all(|s| *s == 0 || *s == 255));
    }
    Image { info, samples }
}

fn image(scene: &Scene, options: PngOptions) -> Image {
    read(&bytes(scene, options).0)
}

const AT_254: PngOptions = PngOptions { dpi: 254.0, colour: PngColour::Grey };

/// A sheet is its size at the dpi, rounded outward as the sheet's pixel grid is (nothing is
/// clipped), and says its dpi, so it prints at the sheet's size: 300 dpi greyscale unless asked,
/// sRGB. A side over the grid's limit is refused with the limit named.
#[test]
fn a_sheet_is_its_size_at_the_dpi_and_records_it() {
    let a4 = scene(p(210.0, 297.0), vec![]);
    assert_eq!(PngOptions::default(), PngOptions { dpi: 300.0, colour: PngColour::Grey });
    let (data, report) = bytes(&a4, PngOptions::default());
    assert_eq!(report.pixels, [2481, 3508]);
    let image = read(&data);
    assert_eq!((image.info.width, image.info.height), (2481, 3508));
    assert_eq!((image.info.color_type, image.info.bit_depth), (png::ColorType::Grayscale, png::BitDepth::Eight));
    let dims = image.info.pixel_dims.unwrap();
    assert_eq!((dims.xppu, dims.yppu, dims.unit), (11811, 11811, png::Unit::Meter));
    assert!(image.info.srgb.is_some());
    assert!(image.samples.iter().all(|s| *s == 255), "blank paper is white");
    assert_eq!(bytes(&a4, PngOptions { dpi: 25.0, ..PngOptions::default() }).1.pixels, [207, 293]);
    assert_eq!(bytes(&scene(p(420.0, 297.0), vec![]), PngOptions::default()).1.pixels, [4961, 3508]);
    let a0 = scene(p(1189.0, 841.0), vec![]);
    let e = paper_png(&a0.prepare().unwrap(), PngOptions { dpi: 1500.0, colour: PngColour::Grey }, &NoPictures, Vec::new()).unwrap_err();
    assert!(e.to_string().contains("over the limit of 65536 px"), "{e}");
}

/// The line and the fill land on their pixels: the line 10 rows deep round row 50 with butt ends,
/// the fill's ring black and its hole white.
#[test]
fn lines_and_fills_land_on_their_pixels() {
    let image = image(&small(), AT_254);
    assert_eq!((image.info.width, image.info.height), (200, 100));
    let grey = |x, y| image.at(x, y)[0];
    for (x, y, value, what) in [
        (100, 45, 0, "the line's top row"),
        (100, 54, 0, "its bottom row"),
        (100, 44, 255, "above it"),
        (100, 55, 255, "below it"),
        (20, 50, 0, "its first column"),
        (19, 50, 255, "before its butt end"),
        (179, 50, 0, "its last column"),
        (180, 50, 255, "after it"),
        (30, 75, 0, "the fill's ring"),
        (50, 75, 255, "the fill's hole"),
        (90, 75, 255, "beside the fill"),
    ] {
        assert_eq!(grey(x, y), value, "{what}, ({x}, {y})");
    }
}

/// Each colour: its PNG type, black ink and white (or no) paper.
#[test]
fn each_colour_has_its_type_ink_and_paper() {
    for (colour, kind, depth, ink, paper) in [
        (PngColour::Grey, png::ColorType::Grayscale, png::BitDepth::Eight, &[0][..], &[255][..]),
        (PngColour::BlackWhite, png::ColorType::Grayscale, png::BitDepth::One, &[0], &[255]),
        (PngColour::Rgb, png::ColorType::Rgb, png::BitDepth::Eight, &[0, 0, 0], &[255, 255, 255]),
        (PngColour::Transparent, png::ColorType::Rgba, png::BitDepth::Eight, &[0, 0, 0, 255], &[0, 0, 0, 0]),
    ] {
        let image = image(&small(), PngOptions { dpi: 254.0, colour });
        assert_eq!((image.info.color_type, image.info.bit_depth), (kind, depth), "{colour:?}");
        assert_eq!(image.at(100, 50), ink, "{colour:?}: the line");
        // Either side of its butt end, bits 3 and 4 of a 1-bit row's third byte.
        assert_eq!((image.at(19, 50), image.at(20, 50)), (paper, ink), "{colour:?}: the line's start");
        assert_eq!(image.at(100, 20), paper, "{colour:?}: the paper");
    }
}

/// A line far thinner than a pixel is a light grey in greyscale, and a full black pixel in black
/// and white, so it doesn't drop out, even between two rows' centres; edges are anti-aliased only
/// where there is grey, and black and white inks a pixel when its centre is inside.
#[test]
fn a_hairline_stays_a_pixel_wide_in_black_and_white() {
    // 50.25 rows from the top: between the centres of rows 49 and 50.
    let hair = scene(p(20.0, 10.0), vec![Item::Stroke { path: vec![line(p(2.0, 4.975), p(18.0, 4.975))], closed: false, width_mm: 0.01 }]);
    let column = |colour| {
        let image = image(&hair, PngOptions { dpi: 254.0, colour });
        (40..60).map(|y| image.at(100, y)[0]).collect::<Vec<u8>>()
    };
    let grey = column(PngColour::Grey);
    assert!(grey.iter().all(|v| *v > 200) && grey.iter().any(|v| *v < 255), "a tenth of a pixel is light grey: {grey:?}");
    let bw = column(PngColour::BlackWhite);
    assert_eq!(bw.iter().filter(|v| **v == 0).count(), 1, "one black pixel across: {bw:?}");
    let edge = image(&small(), PngOptions { dpi: 100.0, colour: PngColour::Grey });
    assert!(edge.samples.iter().any(|v| *v != 0 && *v != 255), "greyscale edges are anti-aliased");
    // A bar a fifth of a pixel wide over column 100's centre: grey in greyscale, black in black and white.
    let bar = scene(p(20.0, 10.0), vec![Item::Fill { region: Region2 { outer: rectangle(p(10.045, 2.0), p(10.065, 8.0)), holes: vec![] } }]);
    let grey = image(&bar, AT_254).at(100, 50)[0];
    assert!(grey > 0 && grey < 255, "under half a pixel is grey: {grey}");
    assert_eq!(image(&bar, PngOptions { dpi: 254.0, colour: PngColour::BlackWhite }).at(100, 50), [0], "its centre is inside");
}

/// A closed outline is joined where it starts, like its other corners, not left with two butt ends.
#[test]
fn a_closed_outline_joins_at_its_start() {
    let square = rectangle(p(2.0, 2.0), p(8.0, 8.0)).segs;
    let outline = scene(p(10.0, 10.0), vec![Item::Stroke { path: square, closed: true, width_mm: 1.0 }]);
    let image = image(&outline, AT_254);
    // A quarter of a millimetre outside each corner, inside the 1 mm line's mitred corner.
    for (x, y, what) in [(17, 82, "the start"), (82, 82, "the second corner"), (82, 17, "the third"), (17, 17, "the fourth")] {
        assert_eq!(image.at(x, y), [0], "{what}, ({x}, {y})");
    }
}

/// Arcs are drawn from lines within 1/8 px: on a 40 mm circle at 600 dpi, every pixel the stroke's
/// middle crosses is black and pixels a pixel outside its 0.5 mm width are white, at 64 angles.
#[test]
fn a_large_circle_is_round_to_the_pixel() {
    let c = p(50.0, 50.0);
    let circle = scene(
        p(100.0, 100.0),
        vec![Item::Stroke { path: vec![Seg2::Arc { center: c, radius: 40.0, start: 0.0, sweep: TAU }], closed: true, width_mm: 0.5 }],
    );
    let image = image(&circle, PngOptions { dpi: 600.0, colour: PngColour::Grey });
    let per_mm = 600.0 / 25.4;
    let pixel = |q: Vec2| image.at((q.x * per_mm) as usize, ((100.0 - q.y) * per_mm) as usize)[0];
    for k in 0..64 {
        let towards = Vec2::from_angle(TAU * f64::from(k) / 64.0);
        assert_eq!(pixel(c + towards * 40.0), 0, "on the circle at {k}/64");
        for r in [40.0 - 0.25 - 1.5 / per_mm, 40.0 + 0.25 + 1.5 / per_mm] {
            assert_eq!(pixel(c + towards * r), 255, "off the circle at {k}/64, r {r}");
        }
    }
}

/// Drawn a few rows at a time or all at once, the file is the same bytes, pictures and all, and
/// each picture row is asked for once; and again the same bytes.
#[test]
fn bands_change_nothing_and_the_bytes_repeat() {
    let mut items = small().groups.remove(0).items;
    items.push(Item::Stroke {
        path: vec![Seg2::Arc { center: p(14.0, 3.0), radius: 2.5, start: 0.3, sweep: 1.5 * FRAC_PI_2 }],
        closed: false,
        width_mm: 0.35,
    });
    items.insert(0, picture(1, p(1.0, 1.0), p(3.0, 2.0), 1.0));
    items.push(picture(2, p(12.0, 5.0), p(16.0, 9.0), 0.7));
    items.push(picture(3, p(8.0, 0.5), p(10.03, 4.5), 1.0));
    let sheet = scene(p(20.0, 10.0), items);
    let prepared = sheet.prepare().unwrap();
    // The rows asked for, by source: a band asks only for its own rows of a picture.
    let rows_asked = |provider: &Provider| {
        let mut rows = [0; 4];
        for (source, _, window) in provider.asked.borrow().iter() {
            rows[*source as usize] += window.y1 - window.y0;
        }
        rows
    };
    for colour in [PngColour::Grey, PngColour::BlackWhite, PngColour::Rgb, PngColour::Transparent] {
        let options = PngOptions { dpi: 300.0, colour };
        let (whole, once) = (&mut Vec::new(), Provider::default());
        write(&prepared, options, &once, &mut *whole, usize::MAX).unwrap();
        let width = read(whole).info.width as usize;
        for band_rows in [1, 3, 7] {
            let (rows, banded) = (&mut Vec::new(), Provider::default());
            write(&prepared, options, &banded, &mut *rows, band_rows * width * 4).unwrap();
            assert_eq!(whole, rows, "{colour:?}, {band_rows} rows a band");
            assert_eq!(rows_asked(&banded), rows_asked(&once), "{colour:?}, {band_rows} rows a band: each picture row once");
        }
        let again = &mut Vec::new();
        paper_png(&prepared, options, &Provider::default(), &mut *again).unwrap();
        assert_eq!(again, whole, "{colour:?}: the same bytes again");
    }
}

/// Out of range resolutions, a curve the walk can't take yet, and more points than the cap are
/// refused, never drawn some other way.
#[test]
fn what_cannot_be_drawn_is_refused() {
    let prepared_small = small();
    let prepared = prepared_small.prepare().unwrap();
    for dpi in [24.9, 2400.1, 0.0, -300.0, f64::NAN, f64::INFINITY] {
        let e = paper_png(&prepared, PngOptions { dpi, colour: PngColour::Grey }, &NoPictures, Vec::new()).unwrap_err();
        assert!(e.to_string().contains("25 to 2400 dpi"), "{dpi}: {e}");
    }
    for dpi in [25.0, 2400.0] {
        assert!(paper_png(&prepared, PngOptions { dpi, colour: PngColour::Grey }, &NoPictures, Vec::new()).is_ok(), "{dpi}");
    }
    let ellipse = scene(
        p(20.0, 10.0),
        vec![Item::Stroke { path: vec![Seg2::Conic { a: p(0.0, 0.0), apex: p(5.0, 5.0), b: p(10.0, 0.0), w: 0.5 }], closed: false, width_mm: 0.35 }],
    );
    let e = paper_png(&ellipse.prepare().unwrap(), PngOptions::default(), &NoPictures, Vec::new()).unwrap_err();
    assert!(e.to_string().contains("not supported yet"), "{e}");
    // 1,000 circles of 100 m at 2400 dpi, each about 19,000 points: over 2^24 in all. A small sheet,
    // so a cap that failed would cost seconds, not hours.
    let big = Loop2::circle(p(0.0, 0.0), 1e5);
    let circles = (0..1000).map(|_| Item::Fill { region: Region2 { outer: big.clone(), holes: vec![] } }).collect();
    let e =
        paper_png(&scene(p(20.0, 10.0), circles).prepare().unwrap(), PngOptions { dpi: 2400.0, colour: PngColour::Grey }, &NoPictures, Vec::new())
            .unwrap_err();
    assert!(e.to_string().contains("16777216 points"), "{e}");
}

/// Pictures by source: 0 a checkerboard of the asked grid's own pixels, 1 red, 2 blue at half
/// alpha, 3 mid grey, 4 noise that doesn't compress, 666 an error; every request is kept, and
/// `short` drops a byte. The SVG tests use it too.
#[derive(Default)]
pub(in crate::paper) struct Provider {
    pub(in crate::paper) asked: RefCell<Vec<(u64, PixelGrid, GridWindow)>>,
    pub(in crate::paper) short: bool,
}

impl Pictures for Provider {
    fn rows(&self, source: u64, grid: &PixelGrid, window: GridWindow) -> std::result::Result<Vec<u8>, String> {
        self.asked.borrow_mut().push((source, *grid, window));
        if source == 666 {
            return Err("boom".into());
        }
        let mut rows = Vec::new();
        for y in window.y0..window.y1 {
            for x in window.x0..window.x1 {
                rows.extend(match source {
                    0 if (x + y) % 2 == 0 => [0, 0, 0, 255],
                    0 => [255, 255, 255, 255],
                    1 => [255, 0, 0, 255],
                    2 => [0, 0, 255, 128],
                    4 => (x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)).wrapping_mul(2_246_822_519).to_le_bytes(),
                    _ => [128, 128, 128, 255],
                });
            }
        }
        if self.short {
            rows.pop();
        }
        Ok(rows)
    }
}

fn picture(source: u64, min: Vec2, max: Vec2, opacity: f64) -> Item {
    Item::Picture(Picture { source, rect: Aabb2 { min, max }, opacity })
}

fn pictured(scene: &Scene, options: PngOptions, pictures: &Provider) -> Image {
    let mut out = Vec::new();
    paper_png(&scene.prepare().unwrap(), options, pictures, &mut out).unwrap();
    read(&out)
}

const RGB_254: PngOptions = PngOptions { dpi: 254.0, colour: PngColour::Rgb };

/// A picture fills exactly its window of the sheet's grid: a 2 × 1 mm picture 3 mm below the
/// top, from x = 4, is columns 40 to 59 and rows 30 to 39 at 254 dpi, and a fill over the same
/// rectangle covers every one of its pixels, so the two register. Also on a sheet 100.8 pixels
/// high, whose grid is anchored at its top edge as the lines are, not at its bottom.
#[test]
fn a_picture_fills_exactly_its_window_and_registers_with_the_lines() {
    for height in [10.0, 10.08] {
        let rect = rectangle(p(4.0, height - 4.0), p(6.0, height - 3.0));
        let red = picture(1, p(4.0, height - 4.0), p(6.0, height - 3.0), 1.0);
        let image = pictured(&scene(p(20.0, height), vec![red.clone()]), RGB_254, &Provider::default());
        for (x, y, colour, what) in [
            (40, 30, [255, 0, 0], "its top-left pixel"),
            (59, 39, [255, 0, 0], "its bottom-right pixel"),
            (39, 35, [255, 255, 255], "left of it"),
            (60, 35, [255, 255, 255], "right of it"),
            (50, 29, [255, 255, 255], "above it"),
            (50, 40, [255, 255, 255], "below it"),
        ] {
            assert_eq!(image.at(x, y), colour, "{height} mm high: {what}, ({x}, {y})");
        }
        let covered = Item::Fill { region: Region2 { outer: rect, holes: vec![] } };
        let image = pictured(&scene(p(20.0, height), vec![red, covered]), RGB_254, &Provider::default());
        assert!(image.samples.chunks(3).all(|c| c[1] == c[0]), "{height} mm high: no red left beside the fill's edges");
    }
}

/// Paint goes on in the scene's order: a fill before a picture is under it, one after it is over it.
#[test]
fn pictures_keep_their_place_in_the_paint_order() {
    let red = || picture(1, p(4.0, 6.0), p(6.0, 7.0), 1.0);
    let fill = || Item::Fill { region: Region2 { outer: rectangle(p(3.0, 5.0), p(7.0, 8.0)), holes: vec![] } };
    let under = pictured(&scene(p(20.0, 10.0), vec![fill(), red()]), RGB_254, &Provider::default());
    assert_eq!(under.at(50, 35), [255, 0, 0], "the picture over the fill");
    assert_eq!(under.at(35, 35), [0, 0, 0], "the fill around it");
    let over = pictured(&scene(p(20.0, 10.0), vec![red(), fill()]), RGB_254, &Provider::default());
    assert_eq!(over.at(50, 35), [0, 0, 0], "the fill over the picture");
}

/// Opacity scales a picture's alpha: red at half over white paper is pink; on transparent paper
/// the pixel keeps the half alpha.
#[test]
fn opacity_scales_the_alpha() {
    let half = scene(p(20.0, 10.0), vec![picture(1, p(4.0, 6.0), p(6.0, 7.0), 0.5)]);
    assert_eq!(pictured(&half, RGB_254, &Provider::default()).at(50, 35), [255, 128, 128]);
    let clear = pictured(&half, PngOptions { dpi: 254.0, colour: PngColour::Transparent }, &Provider::default());
    assert_eq!(clear.at(50, 35), [255, 0, 0, 128]);
    let blue = scene(p(20.0, 10.0), vec![picture(2, p(4.0, 6.0), p(6.0, 7.0), 1.0)]);
    assert_eq!(pictured(&blue, RGB_254, &Provider::default()).at(50, 35), [127, 127, 255], "its own half alpha");
}

/// Up to 1200 dpi a picture is asked for on the sheet's own grid. Above it, on the grid of half
/// the dpi at 2400, each of whose pixels fills 2 × 2 of the sheet's.
#[test]
fn above_1200_dpi_a_coarser_pictures_pixels_are_copied() {
    let checks = scene(p(20.0, 10.0), vec![picture(0, p(4.0, 4.0), p(5.0, 5.0), 1.0)]);
    for (dpi, k) in [(1200.0, 1), (2400.0, 2)] {
        let provider = Provider::default();
        let image = pictured(&checks, PngOptions { dpi, colour: PngColour::Rgb }, &provider);
        let sheet = PixelGrid::sheet(p(20.0, 10.0), dpi).unwrap();
        let window = sheet.window(Aabb2 { min: p(4.0, 4.0), max: p(5.0, 5.0) }).unwrap();
        let asked = provider.asked.borrow();
        assert_eq!(asked.len(), 1, "one band, one request");
        assert!((asked[0].1.pitch - sheet.pitch * f64::from(k)).abs() < 1e-12, "{dpi}: asked at {} mm", asked[0].1.pitch);
        for y in window.y0..window.y1 {
            for x in window.x0..window.x1 {
                let black = (x / k + y / k) % 2 == 0;
                assert_eq!(image.at(x as usize, y as usize), if black { [0, 0, 0] } else { [255, 255, 255] }, "{dpi}: ({x}, {y})");
            }
        }
    }
}

/// In black and white a picture is dithered at its place on the sheet: mid grey is half black,
/// whatever the band; a line over it stays solid; the report says so.
#[test]
fn black_and_white_dithers_pictures_and_keeps_lines_solid() {
    // 16 × 16 pixels at 254 dpi, from column 8 and row 8.
    let grey = picture(3, p(0.8, 7.6), p(2.4, 9.2), 1.0);
    let line = Item::Stroke { path: vec![line(p(0.0, 8.0), p(20.0, 8.0))], closed: false, width_mm: 0.2 };
    let sheet = scene(p(20.0, 10.0), vec![grey, line]);
    let options = PngOptions { dpi: 254.0, colour: PngColour::BlackWhite };
    let prepared = sheet.prepare().unwrap();
    let (whole, rows) = (&mut Vec::new(), &mut Vec::new());
    let report = write(&prepared, options, &Provider::default(), &mut *whole, usize::MAX).unwrap();
    write(&prepared, options, &Provider::default(), &mut *rows, 1).unwrap();
    assert_eq!(whole, rows, "bands change nothing");
    assert!(report.dithered);
    let image = read(whole);
    let black = (8..24).flat_map(|y| (8..24).map(move |x| (x, y))).filter(|&(x, y)| y != 19 && y != 20 && image.at(x, y)[0] == 0).count();
    assert_eq!(black, 14 * 16 / 2, "half of mid grey is black, away from the line");
    assert!((0..200).all(|x| image.at(x, 19)[0] == 0 && image.at(x, 20)[0] == 0), "the line is solid across the picture");
    assert!(!bytes(&small(), options).1.dithered, "nothing to dither");
}

/// A provider's pixels are checked: too few bytes, or an error, refuse the PNG with the source
/// named; no provider refuses any picture; a picture off the sheet asks for nothing.
#[test]
fn a_providers_pixels_are_checked() {
    let sheet = |source| scene(p(20.0, 10.0), vec![picture(source, p(4.0, 6.0), p(6.0, 7.0), 1.0)]);
    let refused = |s: &Scene, provider: &dyn Pictures| paper_png(&s.prepare().unwrap(), RGB_254, provider, Vec::new()).unwrap_err().to_string();
    let short = refused(&sheet(1), &Provider { short: true, ..Provider::default() });
    assert!(short.contains("picture 1: 799 bytes where 800 were asked for"), "{short}");
    assert!(refused(&sheet(666), &Provider::default()).contains("picture 666: boom"));
    assert!(refused(&sheet(1), &NoPictures).contains("no pixels for picture 1"));
    let provider = Provider::default();
    pictured(&scene(p(20.0, 10.0), vec![picture(1, p(30.0, 30.0), p(40.0, 40.0), 1.0)]), RGB_254, &provider);
    assert!(provider.asked.borrow().is_empty());
}

/// One authored 100 mm length is already placed on paper at 1:2 and 1:4.
/// These are consumer fixtures, not a model-view transform or a new view kind.
fn mixed_scale_paper_pair() -> Scene {
    Scene {
        size_mm: p(100.0, 80.0),
        groups: [(101, p(10.0, 20.0), p(60.0, 20.0)), (202, p(10.0, 40.0), p(35.0, 40.0))]
            .into_iter()
            .map(|(object, a, b)| Group {
                object,
                kind: ObjectKind::DrawingBorder,
                items: vec![Item::Stroke { path: vec![line(a, b)], closed: false, width_mm: 0.2 }],
            })
            .collect(),
    }
}

/// Both real writers consume this same checked scene; no picture source is needed.
fn svg_png_pair(scene: &Scene) -> (String, Image, PngReport) {
    let prepared = scene.prepare().unwrap();
    let (svg, svg_report) = crate::paper::paper_svg(&prepared, &NoPictures, 300.0).unwrap();
    let mut png = Vec::new();
    let png_report = paper_png(&prepared, AT_254, &NoPictures, &mut png).unwrap();
    assert_eq!(svg_report, png_report.grid, "both consumers use the same number-grid rules");
    let image = read(&png);
    assert_eq!((image.info.color_type, image.info.bit_depth), (png::ColorType::Grayscale, png::BitDepth::Eight));
    (String::from_utf8(svg).unwrap(), image, png_report)
}

/// 254 dpi is 10 px/mm: 50/25 mm occupy 500/250 px while both 0.2 mm widths stay 2 px.
/// All coordinates are hand-derived from the preplaced fixture, not recorded output.
#[test]
fn svg_and_png_keep_mixed_scale_paper_lengths_and_fixed_width() {
    let (svg, image, report) = svg_png_pair(&mixed_scale_paper_pair());
    assert!(svg.contains("width=\"100mm\" height=\"80mm\" viewBox=\"0 0 100 80\""));
    assert_eq!(svg.matches("<g data-object=").count(), 2);
    assert_eq!(svg.matches("<path ").count(), 2);
    for path in [
        "<path stroke-width=\"0.2\" d=\"M10 60L60 60\"/>",
        "<path stroke-width=\"0.2\" d=\"M10 40L35 40\"/>",
    ] {
        assert_eq!(svg.matches(path).count(), 1, "{path}");
    }
    let first = svg.find("<g data-object=\"101\" data-kind=\"drawing_border\">").unwrap();
    let second = svg.find("<g data-object=\"202\" data-kind=\"drawing_border\">").unwrap();
    assert!(first < second, "group order is retained");
    assert_eq!(report.pixels, [1000, 800]);
    assert_eq!((image.info.width, image.info.height), (1000, 800));
    // Butt ends are [100,600) and [100,350). After the y flip, their two complete
    // black rows are 599/600 and 399/400. Every other pixel is white.
    for y in 0..800 {
        for x in 0..1000 {
            let ink = ((599..601).contains(&y) && (100..600).contains(&x)) || ((399..401).contains(&y) && (100..350).contains(&x));
            assert_eq!(image.at(x, y), if ink { &[0][..] } else { &[255][..] }, "pixel ({x},{y})");
        }
    }
}

/// The vector sheet is exactly 100.08 by 80.08 mm. At 254 dpi its 1000.8 by
/// 800.8 pixel extent needs an outward 1001 by 801 image. Ink reaches both last
/// partial pixels; rounding away that row/column or moving the origin loses it.
#[test]
fn svg_and_png_keep_fractional_sheet_edges_on_the_outward_grid() {
    let sheet = scene(
        p(100.08, 80.08),
        vec![Item::Fill { region: Region2 { outer: rectangle(p(99.0, 0.0), p(100.08, 1.0)), holes: vec![] } }],
    );
    let (svg, image, report) = svg_png_pair(&sheet);
    assert!(svg.contains("width=\"100.08mm\" height=\"80.08mm\" viewBox=\"0 0 100.08 80.08\""));
    assert_eq!(svg.matches("<path ").count(), 1);
    assert!(svg.contains("<path fill=\"#000\" stroke=\"none\" fill-rule=\"evenodd\" d=\"M99 80.08L100.08 80.08L100.08 79.08L99 79.08Z\"/>"));
    assert_eq!(report.pixels, [1001, 801]);
    assert_eq!((image.info.width, image.info.height), (1001, 801));
    assert_eq!(image.at(995, 795), [0], "inside the fill");
    assert_eq!(image.at(989, 795), [255], "left of the fill");
    assert_eq!(image.at(995, 789), [255], "above the fill");
    // The last column/row have 0.8 coverage and the corner 0.64. They are
    // partial dark pixels, not fully black and not white; no exact AA byte is
    // assumed. The existing PNG tests use the same mid-grey ink distinction.
    for (x, y) in [(1000, 795), (995, 800), (1000, 800)] {
        let grey = image.at(x, y)[0];
        assert!(grey > 0 && grey < 128, "ink in the last partial pixel ({x},{y}): {grey}");
    }
    let top = image.at(995, 790)[0];
    assert!(top > 128 && top < 255, "the first partial row has only 0.2 coverage: {top}");
}

const PICTURE_ACCEPTANCE_DPI: f64 = 25.4;

/// Original pixels, top row first: each atlas corner and its middle differ.
const PAPER_ATLAS_RGBA: [[u8; 4]; 6] = [
    [255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255],
    [0, 255, 255, 255], [255, 0, 255, 255], [255, 255, 0, 255],
];
const PAPER_ALPHA_RGBA: [[u8; 4]; 4] = [
    [255, 0, 0, 255], [0, 255, 0, 128],
    [0, 0, 255, 0], [255, 255, 255, 255],
];
const PAPER_BLUE_RGBA: [[u8; 4]; 4] = [[0, 0, 255, 255]; 4];

/// At 25.4 dpi one pixel is exactly one millimetre. No image resampling oracle.
fn paper_picture_pixels(source: u64) -> (Aabb2, [usize; 2], &'static [[u8; 4]]) {
    let (min, max, size, rgba) = match source {
        901 => (p(3.0, 3.0), p(6.0, 5.0), [3, 2], &PAPER_ATLAS_RGBA[..]),
        902 => (p(-1.0, 7.0), p(2.0, 9.0), [3, 2], &PAPER_ATLAS_RGBA[..]),
        903 => (p(6.0, 2.0), p(8.0, 4.0), [2, 2], &PAPER_ALPHA_RGBA[..]),
        904 => (p(6.0, 2.0), p(8.0, 4.0), [2, 2], &PAPER_BLUE_RGBA[..]),
        _ => panic!("unknown original acceptance picture {source}"),
    };
    (Aabb2 { min, max }, size, rgba)
}

fn paper_picture_item(source: u64, opacity: f64) -> Item {
    let (rect, _, _) = paper_picture_pixels(source);
    Item::Picture(Picture { source, rect, opacity })
}

#[derive(Default)]
struct PaperPicturePixels {
    asked: RefCell<Vec<(u64, PixelGrid, GridWindow)>>,
}

impl Pictures for PaperPicturePixels {
    fn rows(&self, source: u64, grid: &PixelGrid, window: GridWindow) -> std::result::Result<Vec<u8>, String> {
        self.asked.borrow_mut().push((source, *grid, window));
        let (rect, size, rgba) = paper_picture_pixels(source);
        let mut out = Vec::new();
        // A provider reads the requested world grid: PNG asks on the sheet's
        // grid, SVG on the picture's own. Both sample the same original pixels.
        for y in window.y0..window.y1 {
            let (low, high) = grid.row(y);
            let row = (rect.max.y - (low + high) * 0.5).floor() as usize;
            for x in window.x0..window.x1 {
                let (low, high) = grid.column(x);
                let col = ((low + high) * 0.5 - rect.min.x).floor() as usize;
                if col >= size[0] || row >= size[1] {
                    return Err(format!("outside original {source} at ({col},{row})"));
                }
                out.extend_from_slice(&rgba[row * size[0] + col]);
            }
        }
        Ok(out)
    }
}

/// Both exports consume one Prepared, with the actual Pictures interface.
fn paper_picture_exports(sheet: &Scene, pixels: &PaperPicturePixels, colour: PngColour) -> (String, Image) {
    let prepared = sheet.prepare().unwrap();
    let (svg, svg_report) = crate::paper::paper_svg(&prepared, pixels, PICTURE_ACCEPTANCE_DPI).unwrap();
    let mut png = Vec::new();
    let report = paper_png(&prepared, PngOptions { dpi: PICTURE_ACCEPTANCE_DPI, colour }, pixels, &mut png).unwrap();
    assert_eq!(svg_report, report.grid);
    assert_eq!(report.pixels, [10, 8]);
    (String::from_utf8(svg).unwrap(), read(&png))
}

/// Decode the actual embedded PNG, not a separately generated reference file.
fn paper_embedded_images(svg: &str) -> Vec<(&str, Image)> {
    svg.split("<image ").skip(1).map(|rest| {
        let tag = rest.split_once("/>").unwrap().0;
        let encoded = tag.split_once("href=\"data:image/png;base64,").unwrap().1.split('"').next().unwrap();
        let bytes = solidcraft_doc::canvas::base64_decode(encoded).unwrap();
        (tag, read(&bytes))
    }).collect()
}

/// A non-square, asymmetric colour atlas distinguishes y reversal, reflection,
/// transposition and placement. PNG starts at (3,3); SVG embeds exactly 3 by 2.
#[test]
fn svg_and_png_place_the_same_asymmetric_original_picture() {
    let sheet = scene(p(10.0, 8.0), vec![paper_picture_item(901, 1.0)]);
    let pixels = PaperPicturePixels::default();
    let (svg, png) = paper_picture_exports(&sheet, &pixels, PngColour::Rgb);
    let embedded = paper_embedded_images(&svg);
    assert_eq!(embedded.len(), 1);
    let (tag, image) = &embedded[0];
    assert!(tag.starts_with("x=\"3\" y=\"3\" width=\"3\" height=\"2\" preserveAspectRatio=\"none\""));
    assert_eq!((image.info.width, image.info.height, image.info.color_type), (3, 2, png::ColorType::Rgba));
    for y in 0..8 {
        for x in 0..10 {
            let expected = if (3..6).contains(&x) && (3..5).contains(&y) {
                &PAPER_ATLAS_RGBA[(y - 3) * 3 + x - 3][..3]
            } else { &[255, 255, 255][..] };
            assert_eq!(png.at(x, y), expected, "sheet ({x},{y})");
        }
    }
    for y in 0..2 {
        for x in 0..3 {
            assert_eq!(image.at(x, y), PAPER_ATLAS_RGBA[y * 3 + x]);
        }
    }
    let asked = pixels.asked.borrow();
    assert_eq!(asked.len(), 2, "one small image request per export");
    assert_eq!((asked[0].2.x0, asked[0].2.y0, asked[0].2.x1, asked[0].2.y1), (0, 0, 3, 2));
    assert_eq!((asked[1].2.x0, asked[1].2.y0, asked[1].2.x1, asked[1].2.y1), (3, 3, 6, 5));
}

/// Cropping left and above the sheet retains the atlas's bottom middle/right
/// pixels (magenta/yellow), not its top-left pixels or a stretched whole image.
#[test]
fn svg_and_png_crop_an_original_picture_without_resetting_its_pixel_origin() {
    let sheet = scene(p(10.0, 8.0), vec![paper_picture_item(902, 1.0)]);
    let pixels = PaperPicturePixels::default();
    let (svg, png) = paper_picture_exports(&sheet, &pixels, PngColour::Rgb);
    let embedded = paper_embedded_images(&svg);
    assert_eq!(embedded.len(), 1);
    let (tag, image) = &embedded[0];
    assert!(tag.starts_with("x=\"0\" y=\"0\" width=\"2\" height=\"1\" preserveAspectRatio=\"none\""));
    assert_eq!((image.info.width, image.info.height), (2, 1));
    assert_eq!(image.at(0, 0), PAPER_ATLAS_RGBA[4]);
    assert_eq!(image.at(1, 0), PAPER_ATLAS_RGBA[5]);
    for y in 0..8 {
        for x in 0..10 {
            let expected = match (x, y) {
                (0, 0) => &[255, 0, 255][..],
                (1, 0) => &[255, 255, 0][..],
                _ => &[255, 255, 255][..],
            };
            assert_eq!(png.at(x, y), expected, "cropped sheet ({x},{y})");
        }
    }
    let asked = pixels.asked.borrow();
    assert_eq!(asked.len(), 2);
    assert_eq!((asked[0].2.x0, asked[0].2.y0, asked[0].2.x1, asked[0].2.y1), (1, 1, 3, 2));
    assert_eq!((asked[1].2.x0, asked[1].2.y0, asked[1].2.x1, asked[1].2.y1), (0, 0, 2, 1));
}

/// Straight source alpha and item opacity multiply. Two Pictures in different
/// groups use source-over in group order, independently of image colour order.
#[test]
fn original_picture_alpha_and_opacity_compose_across_groups_in_paint_order() {
    let alpha = paper_picture_item(903, 0.5);
    let (svg, clear) = paper_picture_exports(&scene(p(10.0, 8.0), vec![alpha.clone()]), &PaperPicturePixels::default(), PngColour::Transparent);
    let embedded = paper_embedded_images(&svg);
    assert_eq!(embedded.len(), 1);
    assert!(embedded[0].0.contains(" opacity=\"0.5\""));
    let expected_clear = [[255, 0, 0, 128], [0, 255, 0, 64], [0, 0, 0, 0], [255, 255, 255, 128]];
    for y in 0..2 {
        for x in 0..2 {
            assert_eq!(clear.at(6 + x, 4 + y), expected_clear[y * 2 + x]);
            assert_eq!(embedded[0].1.at(x, y), PAPER_ALPHA_RGBA[y * 2 + x], "SVG keeps straight pixels; opacity stays on the image");
        }
    }
    assert_eq!(clear.at(0, 0), [0, 0, 0, 0]);
    let expected_over_blue = [[128, 0, 128], [0, 64, 191], [0, 0, 255], [128, 128, 255]];
    for reverse in [false, true] {
        let mut groups = vec![
            Group { object: 401, kind: ObjectKind::DrawingBorder, items: vec![paper_picture_item(904, 1.0)] },
            Group { object: 402, kind: ObjectKind::DrawingBorder, items: vec![alpha.clone()] },
        ];
        if reverse { groups.reverse(); }
        let (svg, png) = paper_picture_exports(&Scene { size_mm: p(10.0, 8.0), groups }, &PaperPicturePixels::default(), PngColour::Rgb);
        let first = svg.find(if reverse { "<g data-object=\"402\"" } else { "<g data-object=\"401\"" }).unwrap();
        let last = svg.find(if reverse { "<g data-object=\"401\"" } else { "<g data-object=\"402\"" }).unwrap();
        assert!(first < last);
        let embedded = paper_embedded_images(&svg);
        assert_eq!(embedded.len(), 2);
        for (index, (_, image)) in embedded.iter().enumerate() {
            let expected = match (reverse, index) {
                (true, 0) | (false, 1) => &PAPER_ALPHA_RGBA,
                _ => &PAPER_BLUE_RGBA,
            };
            assert_eq!((image.info.width, image.info.height), (2, 2));
            for y in 0..2 {
                for x in 0..2 {
                    assert_eq!(image.at(x, y), expected[y * 2 + x], "embedded order, reverse={reverse}, image={index}");
                }
            }
        }
        for y in 0..2 {
            for x in 0..2 {
                let expected = if reverse { [0, 0, 255] } else { expected_over_blue[y * 2 + x] };
                assert_eq!(png.at(6 + x, 4 + y), expected, "reverse={reverse}, ({x},{y})");
            }
        }
        assert_eq!(png.at(0, 0), [255, 255, 255]);
    }
}
