//! 2D Geometric primitives for the kernel UI compositor.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Point { x, y }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const ZERO: Size = Size { width: 0, height: 0 };

    pub const fn new(width: u32, height: u32) -> Self {
        Size { width, height }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const ZERO: Rect = Rect { x: 0, y: 0, width: 0, height: 0 };

    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Rect { x, y, width, height }
    }

    #[inline]
    pub fn right(&self) -> i32 {
        self.x.saturating_add(self.width as i32)
    }

    #[inline]
    pub fn bottom(&self) -> i32 {
        self.y.saturating_add(self.height as i32)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn contains_point(&self, p: Point) -> bool {
        p.x >= self.x && p.x < self.right() && p.y >= self.y && p.y < self.bottom()
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    pub fn intersection(&self, other: &Rect) -> Option<Rect> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.right().min(other.right());
        let y2 = self.bottom().min(other.bottom());

        if x2 > x1 && y2 > y1 {
            Some(Rect::new(x1, y1, (x2 - x1) as u32, (y2 - y1) as u32))
        } else {
            None
        }
    }

    pub fn union(&self, other: &Rect) -> Rect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }

        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = self.right().max(other.right());
        let y2 = self.bottom().max(other.bottom());

        Rect::new(x1, y1, (x2 - x1) as u32, (y2 - y1) as u32)
    }

    pub fn inset(&self, dx: i32, dy: i32) -> Rect {
        let double_dx = (dx * 2) as u32;
        let double_dy = (dy * 2) as u32;

        let new_w = self.width.saturating_sub(double_dx);
        let new_h = self.height.saturating_sub(double_dy);

        Rect::new(self.x + dx, self.y + dy, new_w, new_h)
    }

    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.width, self.height)
    }
}
