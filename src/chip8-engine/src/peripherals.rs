pub const DISPLAY_WIDTH: usize = 64;
pub const DISPLAY_HEIGHT: usize = 32;
pub const DISPLAY_PIXELS: usize = DISPLAY_WIDTH * DISPLAY_HEIGHT;
pub const SUPERCHIP_DISPLAY_WIDTH: usize = 128;
pub const SUPERCHIP_DISPLAY_HEIGHT: usize = 64;
pub const MAX_DISPLAY_PIXELS: usize = SUPERCHIP_DISPLAY_WIDTH * SUPERCHIP_DISPLAY_HEIGHT;
const BITPLANES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisplayMode {
    LowResolution,
    HighResolution,
}

impl DisplayMode {
    pub(crate) const fn dimensions(self) -> (usize, usize) {
        match self {
            Self::LowResolution => (DISPLAY_WIDTH, DISPLAY_HEIGHT),
            Self::HighResolution => (SUPERCHIP_DISPLAY_WIDTH, SUPERCHIP_DISPLAY_HEIGHT),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Framebuffer {
    pixels: [u8; MAX_DISPLAY_PIXELS],
    planes: [[u8; MAX_DISPLAY_PIXELS]; BITPLANES],
    mode: DisplayMode,
}

impl Default for Framebuffer {
    fn default() -> Self {
        Self {
            pixels: [0; MAX_DISPLAY_PIXELS],
            planes: [[0; MAX_DISPLAY_PIXELS]; BITPLANES],
            mode: DisplayMode::LowResolution,
        }
    }
}

impl Framebuffer {
    pub(crate) fn clear(&mut self) {
        self.pixels.fill(0);
        self.planes.iter_mut().for_each(|plane| plane.fill(0));
    }

    pub(crate) fn clear_planes(&mut self, planes: u8) {
        for (index, plane) in self.planes.iter_mut().enumerate() {
            if planes & (1 << index) != 0 {
                plane.fill(0);
            }
        }
        self.refresh();
    }

    pub(crate) fn set_mode(&mut self, mode: DisplayMode) {
        self.mode = mode;
        self.clear();
    }

    pub(crate) const fn dimensions(&self) -> (usize, usize) {
        self.mode.dimensions()
    }

    pub(crate) fn pixels(&self) -> &[u8] {
        let (width, height) = self.dimensions();
        &self.pixels[..width * height]
    }

    /// XOR a sprite at `(x, y)`, returning whether an illuminated pixel was erased.
    pub(crate) fn draw(&mut self, x: u8, y: u8, sprite: &[u8], wrap: bool, planes: u8) -> bool {
        let (width, height) = self.dimensions();
        let mut collision = false;
        for (row, byte) in sprite.iter().copied().enumerate() {
            let py = usize::from(y) + row;
            if !wrap && py >= height {
                continue;
            }
            let py = py % height;
            for bit in 0..8 {
                if byte & (0x80 >> bit) == 0 {
                    continue;
                }
                let px = usize::from(x) + bit;
                if !wrap && px >= width {
                    continue;
                }
                let index = py * width + (px % width);
                collision |= self.toggle(index, planes);
            }
        }
        self.refresh();
        collision
    }

    pub(crate) fn draw_16x16(
        &mut self,
        x: u8,
        y: u8,
        sprite: &[u8],
        wrap: bool,
        planes: u8,
    ) -> bool {
        debug_assert_eq!(sprite.len(), 32);
        let (width, height) = self.dimensions();
        let mut collision = false;
        for row in 0..16 {
            let bits = u16::from_be_bytes([sprite[row * 2], sprite[row * 2 + 1]]);
            let py = usize::from(y) + row;
            if !wrap && py >= height {
                continue;
            }
            let py = py % height;
            for bit in 0..16 {
                if bits & (0x8000 >> bit) == 0 {
                    continue;
                }
                let px = usize::from(x) + bit;
                if !wrap && px >= width {
                    continue;
                }
                let index = py * width + (px % width);
                collision |= self.toggle(index, planes);
            }
        }
        self.refresh();
        collision
    }

    pub(crate) fn scroll_down(&mut self, rows: usize, planes: u8) {
        self.scroll_vertical(rows, true, planes);
    }
    pub(crate) fn scroll_up(&mut self, rows: usize, planes: u8) {
        self.scroll_vertical(rows, false, planes);
    }
    fn scroll_vertical(&mut self, rows: usize, down: bool, planes: u8) {
        let (width, height) = self.dimensions();
        for plane in self.selected_planes_mut(planes) {
            let pixels = &mut plane[..width * height];
            if rows >= height {
                pixels.fill(0);
                continue;
            }
            if down {
                pixels.copy_within(..(height - rows) * width, rows * width);
                pixels[..rows * width].fill(0);
            } else {
                pixels.copy_within(rows * width.., 0);
                pixels[(height - rows) * width..].fill(0);
            }
        }
        self.refresh();
    }

    pub(crate) fn scroll_right(&mut self, columns: usize, planes: u8) {
        self.scroll_horizontal(columns, true, planes);
    }

    pub(crate) fn scroll_left(&mut self, columns: usize, planes: u8) {
        self.scroll_horizontal(columns, false, planes);
    }

    fn scroll_horizontal(&mut self, columns: usize, right: bool, planes: u8) {
        let (width, height) = self.dimensions();
        for plane in self.selected_planes_mut(planes) {
            for row in plane[..width * height].chunks_exact_mut(width) {
                if columns >= width {
                    row.fill(0);
                } else if right {
                    row.copy_within(..width - columns, columns);
                    row[..columns].fill(0);
                } else {
                    row.copy_within(columns.., 0);
                    row[width - columns..].fill(0);
                }
            }
        }
        self.refresh();
    }

    fn toggle(&mut self, index: usize, planes: u8) -> bool {
        let mut collision = false;
        for (plane_index, plane) in self.planes.iter_mut().enumerate() {
            if planes & (1 << plane_index) != 0 {
                collision |= plane[index] != 0;
                plane[index] ^= 1;
            }
        }
        collision
    }
    fn selected_planes_mut(
        &mut self,
        planes: u8,
    ) -> impl Iterator<Item = &mut [u8; MAX_DISPLAY_PIXELS]> {
        self.planes
            .iter_mut()
            .enumerate()
            .filter_map(move |(index, plane)| (planes & (1 << index) != 0).then_some(plane))
    }
    fn refresh(&mut self) {
        for (index, output) in self.pixels.iter_mut().enumerate() {
            *output = self
                .planes
                .iter()
                .enumerate()
                .fold(0, |color, (plane, pixels)| color | (pixels[index] << plane));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor_drawing_reports_collision() {
        let mut display = Framebuffer::default();
        assert!(!display.draw(0, 0, &[0b1000_0000], true, 1));
        assert_eq!(display.pixels()[0], 1);
        assert!(display.draw(0, 0, &[0b1000_0000], true, 1));
        assert_eq!(display.pixels()[0], 0);
    }

    #[test]
    fn clipped_drawing_does_not_wrap() {
        let mut display = Framebuffer::default();
        display.draw(63, 31, &[0b1100_0000, 0b1000_0000], false, 1);
        assert_eq!(display.pixels()[DISPLAY_PIXELS - 1], 1);
        assert_eq!(display.pixels()[31 * DISPLAY_WIDTH], 0);
        assert_eq!(display.pixels()[0], 0);
    }

    #[test]
    fn superchip_scrolls_and_switches_resolution() {
        let mut display = Framebuffer::default();
        display.set_mode(DisplayMode::HighResolution);
        assert_eq!(display.dimensions(), (128, 64));
        display.draw(0, 0, &[0x80], false, 1);
        display.scroll_right(4, 1);
        display.scroll_down(2, 1);
        assert_eq!(display.pixels()[2 * 128 + 4], 1);
        display.scroll_left(4, 1);
        assert_eq!(display.pixels()[2 * 128], 1);
    }

    #[test]
    fn combines_four_selected_bitplanes_into_a_sixteen_colour_pixel() {
        let mut display = Framebuffer::default();
        display.draw(0, 0, &[0x80], false, 0b1111);
        assert_eq!(display.pixels()[0], 0b1111);
    }
}
