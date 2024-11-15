// Is the colour trait implemented for each format
// with each function hanging off the type or off the instance

use educe::Educe;
use num::{Num, NumCast};
use paste::paste;
use std::{
    cell::UnsafeCell,
    fmt,
    marker::PhantomData,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};
use winit::dpi::{PhysicalPosition, PhysicalSize};

// Colours (cell_sim.rs / gravity_sim.rs)
pub const GREEN: Rgba = Rgba::from_rgb(40, 255, 40);
pub const WHITE: Rgba = Rgba::from_rgb(255, 255, 255);
pub const BACKGROUND: Rgba = Rgba::from_rgb(44, 44, 44);

// gravity_sim.rs
pub const MOUSE_DRAWBACK_MULTIPLIER: f64 = 20.0;
pub const PHYSICS_MULTIPLIER: f64 = 2.0;
pub const PHYSICS_RESISTANCE: f64 = 0.999;
pub const INIT_PARTICLES: usize = 0;
pub const GRAV_CONST: f64 = 6.67430e-18; // reduced by 10 for better precision?
pub const CAMERA_RESISTANCE: f64 = 0.97;
pub const CAMERA_SPEED: f64 = 0.1;

// Generic Parameters (*)
pub const INIT_TITLE: &str = "Gravity Sim";
pub const INIT_WIDTH: u32 = 1600 / 2;
pub const INIT_HEIGHT: u32 = 1200 / 2;
pub const INIT_SCALE: u32 = 3;
pub const INIT_DRAW_SIZE: i32 = 8;
pub const SIM_MAX_SCALE: u32 = 10;
pub const MAX_DRAW_SIZE: i32 = 500;

// timing (app.rs)
pub const MOUSE_HOLD_THRESHOLD_MS: u64 = 40;
pub const MOUSE_PRESS_COOLDOWN_MS: u64 = 100;
pub const KEY_COOLDOWN_MS: u64 = 100;
pub const TARGET_FPS: f64 = 60.0;
pub const FRAME_TIME_MS: f64 = 1000.0 / TARGET_FPS;
pub const MS_BUFFER: f64 = 3.0;

/*
macro_rules! impl_vec2_op {
    ($name:ident, $param1:ident, $param2: ident, $op_name:ident) => {
        paste! {
            impl<T: Num + Copy> $name<T>{
                pub fn [<$op_name:lower>]<T2: Num + Copy + Into<T>>(self, rhs: $name<T2>)-> Self
                {
                    Self {
                        $param1: self.$param1.[<$op_name:lower>](rhs.$param1.into()),
                        $param2: self.$param2.[<$op_name:lower>](rhs.$param2.into()),
                    }
                }
                pub fn [<$op_name:lower _sep>]<T2: Num + Copy + Into<T>>(self, p1: T2, p2: T2) -> Self {
                    Self {
                        $param1: self.$param1.[<$op_name:lower>](p1.into()),
                        $param2: self.$param2.[<$op_name:lower>](p2.into()),
                    }
                }
                pub fn [<$op_name:lower _scalar>]<T2: Num + Copy + Into<T>>(self, scalar: T2) -> Self {
                    Self {
                        $param1: self.$param1.[<$op_name:lower>](scalar.into()),
                        $param2: self.$param2.[<$op_name:lower>](scalar.into()),
                    }
                }
            }
            impl<T: Num + Copy + [<$op_name Assign>]> [<$op_name Assign>] for $name<T> {
                fn [<$op_name:lower _assign>](&mut self, rhs: Self) {
                    self.$param1.[<$op_name:lower _assign>](rhs.$param1);
                    self.$param2.[<$op_name:lower _assign>](rhs.$param2);
                }
            }
        }
    };
create_vec2!(GamePos, x, y);
create_vec2!(WindowPos, x, y);
create_vec2!(GameSize, width, height);
create_vec2!(WindowSize, width, height);
// TODO(TOM): implement .to_world() -> WorldPos for game vec2
// region: Impl Vec2 Items
impl<T: Num + Copy> GamePos<T> {
    pub fn to_window(self, scale: T) -> WindowPos<T> {
        WindowPos {
            x: self.x * scale,
            y: self.y * scale,
        }
    }
    pub const fn to_size(self) -> GameSize<T> {
        GameSize {
            width: self.x,
            height: self.y,
        }
    }
}
impl<T: Num + Copy> WindowPos<T> {
    pub fn to_game(self, scale: T) -> GamePos<T> {
        GamePos {
            x: self.x / scale,
            y: self.y / scale,
        }
    }
    pub const fn to_size(self) -> WindowSize<T> {
        WindowSize {
            width: self.x,
            height: self.y,
        }
    }
}
impl<T: Num + Copy> GameSize<T> {
    pub fn to_window(self, scale: T) -> WindowSize<T> {
        WindowSize {
            width: self.width * scale,
            height: self.height * scale,
        }
    }

    pub const fn to_pos(self) -> GamePos<T> {
        GamePos {
            x: self.width,
            y: self.height,
        }
    }
}
impl<T: Num + Copy> WindowSize<T> {
    pub fn to_game(self, scale: T) -> GameSize<T> {
        GameSize {
            width: self.width / scale,
            height: self.height / scale,
        }
    }

    pub const fn to_pos(self) -> WindowPos<T> {
        WindowPos {
            x: self.width,
            y: self.height,
        }
    }
}
// endregion
 */
/*
macro_rules! create_vec2 {
    ($name:ident, $param1:ident, $param2: ident) => {
        #[derive(Educe, Clone, Copy, PartialEq, Eq)]
        #[educe(Debug(named_field = false))]
        pub struct $name<T: Copy> {
            pub $param1: T,
            pub $param2: T,
        }
        impl<T: Num + Copy + ToPrimitive> $name<T> {
            pub fn clamp(self, min: $name<T>, max: $name<T>) -> Self
            where
                T: PartialOrd,
            {
                Self {
                    $param1: num::clamp(self.$param1, min.$param1, max.$param1),
                    $param2: num::clamp(self.$param2, min.$param2, max.$param2),
                }
            }

            pub fn into<T2: Num + Copy + From<T>>(self) -> $name<T2> {
                $name {
                    $param1: self.$param1.into(),
                    $param2: self.$param2.into(),
                }
            }

            pub fn map<T2: Num + Copy, F: Fn(T) -> T2>(self, f: F) -> $name<T2> {
                $name {
                    $param1: f(self.$param1),
                    $param2: f(self.$param2),
                }
            }
        }

        impl_vec2_op!($name, $param1, $param2, Add);
        impl_vec2_op!($name, $param1, $param2, Sub);
        impl_vec2_op!($name, $param1, $param2, Mul);
        impl_vec2_op!($name, $param1, $param2, Div);
        // region: From Implementations
        impl<T: Num + Copy> From<(T, T)> for $name<T> {
            fn from((a, b): (T, T)) -> Self {
                Self {
                    $param1: a,
                    $param2: b,
                }
            }
        }
        impl<T: Num + Copy> From<PhysicalSize<T>> for $name<T> {
            fn from(size: PhysicalSize<T>) -> Self {
                Self {
                    $param1: size.width,
                    $param2: size.height,
                }
            }
        }
        impl<T: Num + Copy> From<PhysicalPosition<T>> for $name<T> {
            fn from(pos: PhysicalPosition<T>) -> Self {
                Self {
                    $param1: pos.x,
                    $param2: pos.y,
                }
            }
        }
        impl<T: Num + Copy> From<$name<T>> for PhysicalSize<T> {
            fn from(size: $name<T>) -> Self {
                Self {
                    width: size.$param1,
                    height: size.$param2,
                }
            }
        }
        impl<T: Num + Copy> From<$name<T>> for PhysicalPosition<T> {
            fn from(pos: $name<T>) -> Self {
                Self {
                    x: pos.$param1,
                    y: pos.$param2,
                }
            }
        }
    };
}
*/

// region: Vec2
pub trait CoordSpace {}
macro_rules! create_coordinate_space {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub struct $name;
        impl CoordSpace for $name {}
    };
}
create_coordinate_space!(ScreenSpace);
create_coordinate_space!(RenderSpace);
create_coordinate_space!(WorldSpace);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Scale<T: Num + Copy + Mul, Src: CoordSpace, Dst: CoordSpace>(T, PhantomData<(Src, Dst)>);
impl<T: Num + Copy + Mul, Src: CoordSpace, Dst: CoordSpace> Scale<T, Src, Dst> {
    pub fn new(val: T) -> Self {
        Self(val, PhantomData)
    }

    pub fn get(&self) -> T {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Vec2<T, U: CoordSpace> {
    pub x: T,
    pub y: T,
    _unit: PhantomData<U>,
}

#[inline]
pub fn vec2<T, U: CoordSpace>(p1: T, p2: T) -> Vec2<T, U> {
    Vec2 {
        x: p1,
        y: p2,
        _unit: PhantomData,
    }
}

impl<T: Num + Copy + NumCast, U: CoordSpace> Vec2<T, U> {
    pub fn clamp(self, min: Vec2<T, U>, max: Vec2<T, U>) -> Vec2<T, U>
    where
        T: PartialOrd,
    {
        Vec2 {
            x: num::clamp(self.x, min.x, max.x),
            y: num::clamp(self.y, min.y, max.y),
            _unit: PhantomData,
        }
    }

    pub fn map<T2, F: Fn(T) -> T2>(self, f: F) -> Vec2<T2, U> {
        Vec2 {
            x: f(self.x),
            y: f(self.y),
            _unit: PhantomData,
        }
    }

    pub fn cast<DstT: NumCast>(self) -> Vec2<DstT, U> {
        Vec2 {
            x: DstT::from(self.x).unwrap(),
            y: DstT::from(self.y).unwrap(),
            _unit: PhantomData,
        }
    }

    pub fn cast_unit<DstU: CoordSpace>(self) -> Vec2<T, DstU> {
        Vec2 {
            x: self.x,
            y: self.y,
            _unit: PhantomData,
        }
    }

    pub fn to_array(self) -> [T; 2] {
        [self.x, self.y]
    }

    pub fn scale<SrcT: Num + Copy + NumCast, Dst: CoordSpace>(
        self,
        scale: Scale<SrcT, U, Dst>,
    ) -> Vec2<T, Dst>
    where
        T: Mul,
    {
        Vec2 {
            x: self.x / T::from(scale.get()).unwrap(),
            y: self.y / T::from(scale.get()).unwrap(),
            _unit: PhantomData,
        }
    }
}

macro_rules! impl_vec2_op {
    ($op_name:ident) => {
        paste! {
            impl<T: $op_name<Output = T> + Copy, U: CoordSpace> $op_name for Vec2<T,U> {
                type Output = Vec2<T, U>;
                fn [<$op_name:lower>](self, rhs: Self) -> Self::Output {
                    Vec2 {
                        x: self.x.[<$op_name:lower>](rhs.x),
                        y: self.y.[<$op_name:lower>](rhs.y),
                        _unit: PhantomData,
                    }
                }
            }
            impl<T: $op_name<Output = T> + Copy, U: CoordSpace> $op_name<T> for Vec2<T,U> {
                type Output = Vec2<T, U>;
                fn [<$op_name:lower>](self, rhs: T) -> Self::Output {
                    Vec2 {
                        x: self.x.[<$op_name:lower>](rhs),
                        y: self.y.[<$op_name:lower>](rhs),
                        _unit: PhantomData,
                    }
                }
            }
            impl<T: [<$op_name Assign>] + Copy, U: CoordSpace> [<$op_name Assign>] for Vec2<T, U> {
                fn [<$op_name:lower _assign>](&mut self, rhs: Vec2<T, U>) {
                    self.x.[<$op_name:lower _assign>](rhs.x);
                    self.y.[<$op_name:lower _assign>](rhs.y);
                }
            }
            impl<T: [<$op_name Assign>] + Copy, U: CoordSpace> [<$op_name Assign>]<T> for Vec2<T, U> {
                fn [<$op_name:lower _assign>](&mut self, rhs: T) {
                    self.x.[<$op_name:lower _assign>](rhs);
                    self.y.[<$op_name:lower _assign>](rhs);
                }
            }
        }
    };
}

impl_vec2_op!(Add);
impl_vec2_op!(Sub);
impl_vec2_op!(Mul);
impl_vec2_op!(Div);
// endregion
// region: Shape
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(dead_code)] // don't match shape, I index into it (app::handle_inputs)
pub enum Shape {
    CircleOutline,
    CircleFill,
    SquareCentered,
    Line,
    Arrow,
    Count,
}

impl Shape {
    pub fn draw<F: FnMut(i32, i32)>(self, size: i32, mut lambda: F) {
        match self {
            Self::CircleOutline => {
                let mut x = 0;
                let mut y = size as i32;
                let mut d = 3 - 2 * size as i32;
                let mut draw_circle = |x, y| {
                    lambda(x, y);
                    lambda(-x, y);
                    lambda(x, -y);
                    lambda(-x, -y);
                    lambda(y, x);
                    lambda(-y, x);
                    lambda(y, -x);
                    lambda(-y, -x);
                };
                draw_circle(x, y);
                while x < y {
                    if d < 0 {
                        d = d + 4 * x + 6;
                    } else {
                        y -= 1;
                        d = d + 4 * (x - y) + 10;
                    }
                    x += 1;
                    draw_circle(x, y);
                }
            }
            Self::CircleFill => {
                let mut x = 0;
                let mut y = size as i32;
                let mut d = 3 - 2 * size as i32;
                let mut draw_line = |x1, x2, y| {
                    for x in x1..x2 {
                        lambda(x, y);
                    }
                };
                let mut draw_circle = |x: i32, y: i32| {
                    draw_line(-x, x, y);
                    draw_line(-x, x, -y);
                    draw_line(-y, y, x);
                    draw_line(-y, y, -x);
                };
                draw_circle(x, y);
                while x < y {
                    if d < 0 {
                        d = d + 4 * x + 6;
                    } else {
                        y -= 1;
                        d = d + 4 * (x - y) + 10;
                    }
                    x += 1;
                    draw_circle(x, y);
                }
            }
            Self::SquareCentered => {
                let half = (size / 2) as i32;
                for y_off in -(half)..(half) {
                    for x_off in -(half)..(half) {
                        lambda(x_off, y_off);
                    }
                }
            }
            Self::Line => {
                // line
                todo!("line algo")
            }
            Self::Arrow => {
                // bresenham's line algorithm. point => len
                // line either side of mouse cursor (arrow-ness)
                todo!("arrow algo")
            }
            Self::Count => {
                panic!("Shape::Count is not a valid shape");
            }
        }
    }
}
// endregion
// region: Rgba
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}
#[allow(dead_code)] // maybe one day I will use this
impl Rgba {
    pub const fn as_u32(self) -> u32 {
        (self.r as u32) << 24 | (self.g as u32) << 16 | (self.b as u32) << 8 | self.a as u32
    }

    pub const fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn from_u32(colour: u32) -> Self {
        Self {
            r: ((colour >> 24) & 0xFF) as u8,
            g: ((colour >> 16) & 0xFF) as u8,
            b: ((colour >> 8) & 0xFF) as u8,
            a: (colour & 0xFF) as u8,
        }
    }
} // endregion

// This is a simple wrapper on UnsafeCell for parallelism. (impl Sync)
// UnsafeCell is an unsafe primitive for interior mutability (bypassing borrow checker)
// UnsafeCell provides no thread safety gurantees, I don't care though so I made this wrapper
pub struct SyncCell<T>(UnsafeCell<T>);
unsafe impl<T: Send> Sync for SyncCell<T> {}
impl<T> SyncCell<T> {
    pub const fn new(val: T) -> Self {
        Self(UnsafeCell::new(val))
    }

    pub fn get(&self) -> &T {
        unsafe { &*self.0.get() }
    }

    pub fn get_mut(&self) -> &mut T {
        unsafe { &mut *self.0.get() }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for SyncCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let item = self.get();
        f.debug_struct("SyncCell").field("Item", item).finish()
    }
}

impl<T: Clone> Clone for SyncCell<T> {
    fn clone(&self) -> Self {
        Self::new(self.get().clone())
    }
}

pub fn fmt_limited_precision<T: fmt::Debug>(x: T, format: &mut fmt::Formatter) -> fmt::Result {
    write!(format, "{x:.2?}") // Specify precision here
}
