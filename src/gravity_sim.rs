use crate::{
    app::InputData,
    frontend::{Frontend, SimData},
    utils::*,
};
use core::f64;
use educe::Educe;
use log::{info, trace};
use num::pow::Pow;
use rayon::prelude::*;
use std::{
    mem::transmute,
    ops::{Add, Div, Mul, Sub},
    time::{Duration, Instant},
};
use winit::keyboard::KeyCode;

#[derive(Educe, Clone, Copy)]
#[educe(Debug)]
struct Particle {
    #[educe(Debug(method(fmt_limited_precision)))]
    pos: Vec2<f64, WorldSpace>,
    #[educe(Debug(method(fmt_limited_precision)))]
    vel: Vec2<f64, WorldSpace>,
    #[educe(Debug(method(fmt_limited_precision)))]
    mass: f64,
    #[educe(Debug(method(fmt_limited_precision)))]
    radius: f64,
}

#[derive(Debug, Clone, Copy)]
struct State {
    frame: usize,
    draw_size: i32,
    draw_shape: Shape,
    scale: Scale<i32, ScreenSpace, RenderSpace>,
    running: bool,
    step_sim: bool,
    mouse: Vec2<f64, ScreenSpace>,
}

#[derive(Educe, Clone)]
#[educe(Debug)]
pub struct GravitySim {
    state: State,
    #[educe(Debug(ignore))]
    prev_state: State,

    // thread_pool: ThreadPool,
    window_size: Vec2<i32, ScreenSpace>,
    sim_size: Vec2<i32, RenderSpace>,
    camera: Vec2<f64, WorldSpace>, // describes the top left of the viewport.
    camera_vel: Vec2<f64, WorldSpace>,

    #[educe(Debug(ignore))]
    bufs: [Vec<SyncCell<u8>>; 2],
    front_buffer: usize,
    #[educe(Debug(ignore))]
    particles: Vec<SyncCell<Particle>>,
}

impl Frontend for GravitySim {
    // region: Utility
    fn get_sim_data(&self) -> SimData {
        let buf = &self.bufs[self.front_buffer];
        let buf_slice = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast(), buf.len()) };
        SimData {
            buf: buf_slice,
            size: self.sim_size.cast(),
            frame: self.state.frame,
        }
    }

    fn get_scale(&self) -> u32 {
        self.state.scale.get() as u32
    }
    // endregion
    // region: Sim Manipultion
    fn resize_sim(&mut self, window_size: Vec2<u32, ScreenSpace>) {
        optick::event!("GravitySim::resize_sim");

        let window_size = window_size.cast();
        let new_sim_size = window_size.scale(self.state.scale);

        assert!(
            new_sim_size.x == window_size.x / self.state.scale.get(),
            "{new_sim_size:?} != {window_size:?} / {}",
            self.state.scale.get()
        );

        if new_sim_size == self.sim_size {
            trace!("Sim size unchanged, skipping resize. {new_sim_size:?}");
            return;
        }

        let buf_size = (new_sim_size.x * new_sim_size.y * 4) as usize;
        let mut new_buf = Vec::with_capacity(buf_size);
        let mut new_buf_clone = Vec::with_capacity(buf_size);
        for _ in 0..buf_size {
            new_buf.push(SyncCell::new(44));
            new_buf_clone.push(SyncCell::new(44));
        }

        trace!(
            "Resizing sim to: {new_sim_size:?} | {window_size:?} | scale: {:?} | {buf_size}",
            self.state.scale
        );

        self.window_size = window_size;
        self.sim_size = new_sim_size;
        self.bufs = [new_buf, new_buf_clone];
        // don't change particle stuff.
    }

    fn rescale_sim(&mut self, new_scale: u32) {
        self.state.scale = Scale::new(new_scale as i32);
        self.resize_sim(self.window_size.cast::<u32>());
    }
    // endregion
    // region: Update
    fn update(&mut self, inputs: &mut InputData) {
        optick::event!("GravitySim::update");

        self.handle_input_state(inputs);

        // Clear render buffer to gray RGB(44,44,44)
        {
            optick::event!("Resetting texture");
            let buf_ptr = self.bufs[self.front_buffer].as_mut_ptr();
            unsafe {
                // .iter.map prob gets optimized to this, but just in case.
                buf_ptr.write_bytes(44, self.bufs[self.front_buffer].len());
            }
        }

        if self.state.running || self.state.step_sim {
            self.update_physics();
        }

        Self::render_particles(
            &self.bufs[self.front_buffer],
            &self.particles,
            self.sim_size,
            self.camera,
        );

        self.handle_input_renders(inputs);

        if self.state.frame % TARGET_FPS as usize == 0 {
            trace!("Particles: {}", self.particles.len());
        }

        self.prev_state = self.state;
        self.state.step_sim = false;
        self.state.frame += 1;

        //TODO(TOM): sort out & use for multiple frames in flight.
        // self.front_buffer = (self.front_buffer + 1) % 2;
    }
    // endregion
}

//////////////////////////////////////////////////////////////////////////////////////////

impl GravitySim {
    // region: Little ones
    fn create_particle(
        pos: Vec2<f64, WorldSpace>,
        vel: Vec2<f64, WorldSpace>,
        radius: f64,
        density: f64,
    ) -> SyncCell<Particle> {
        SyncCell::new(Particle {
            pos,
            vel,
            mass: f64::consts::PI * 4.0 / 3.0 * radius.pow(3) * density,
            radius,
        })
    }

    fn spawn_particle(&mut self, pos: Vec2<f64, WorldSpace>, vel: Vec2<f64, WorldSpace>) {
        self.particles.push(Self::create_particle(
            pos,
            vel,
            f64::from(self.state.draw_size),
            EARTH_DENSITY,
        ));
    }

    fn init_particles() -> [SyncCell<Particle>; 2] {
        [
            Self::create_particle(vec2(120.0, 120.0), vec2(0.0, 0.0), SUN_RADIUS, SUN_DENSITY),
            Self::create_particle(vec2(320.0, 320.0), vec2(0.0, 0.0), SUN_RADIUS, SUN_DENSITY),
        ]
    }

    fn reset_sim(&mut self) {
        self.particles.clear();
        self.particles.extend_from_slice(&Self::init_particles());
    }

    fn clear_sim(&mut self) {
        self.particles.clear();
    }

    fn write_colour(index: usize, buf: &[SyncCell<u8>], col: Rgba) {
        *buf[index + 0].get_mut() = col.r;
        *buf[index + 1].get_mut() = col.g;
        *buf[index + 2].get_mut() = col.b;
        *buf[index + 3].get_mut() = col.a;
    }

    fn write_to_buf(&mut self, pos: Vec2<i32, RenderSpace>, col: Rgba) {
        let index = 4 * (pos.y * self.sim_size.x + pos.x) as usize;
        let buf = &mut self.bufs[self.front_buffer];
        Self::write_colour(index, buf, col);
    }
    // endregion
    // region: Input Handling
    fn handle_input_state(&mut self, inputs: &mut InputData) {
        optick::event!("Handling Input State");

        let pressed = inputs.mouse_pressed.pos;
        let released = inputs.mouse_released.pos;
        let mouse_pos_world = pressed.scale(self.state.scale).cast_unit().add(self.camera);
        if inputs.was_mouse_dragging() {
            // Draws particle at initial position, give it velocity based on drag distance.
            let pressed_world = pressed.scale(self.state.scale).cast_unit().add(self.camera);
            let game_pos_delta = pressed.sub(released).scale(self.state.scale);

            // TODO(TOM): vary with current scale factor.
            let velocity = game_pos_delta
                .div(self.sim_size.cast())
                .mul(MOUSE_DRAWBACK_MULTIPLIER)
                .cast_unit();

            self.spawn_particle(mouse_pos_world, velocity);
        } else if inputs.was_mouse_pressed() {
            self.spawn_particle(mouse_pos_world, vec2(0.0, 0.0));
        }

        // Toggle simulation on KeySpace
        if inputs.is_pressed(KeyCode::Space) {
            self.state.running = !self.state.running;
            info!("Sim running: {}", self.state.running);
        }
        self.state.step_sim = inputs.is_pressed(KeyCode::ArrowRight);

        // Clear Sim on KeyC
        if inputs.is_pressed(KeyCode::KeyC) {
            self.clear_sim();
        } else if inputs.is_pressed(KeyCode::KeyR) {
            self.reset_sim();
        }

        // Branchless Camera Movement
        self.camera_vel.y -= CAMERA_SPEED * inputs.is_held(KeyCode::KeyW) as i32 as f64;
        self.camera_vel.y += CAMERA_SPEED * inputs.is_held(KeyCode::KeyS) as i32 as f64;
        self.camera_vel.x += CAMERA_SPEED * inputs.is_held(KeyCode::KeyD) as i32 as f64;
        self.camera_vel.x -= CAMERA_SPEED * inputs.is_held(KeyCode::KeyA) as i32 as f64;

        // Branchless Draw Size Change
        self.state.draw_size += inputs.is_pressed(KeyCode::ArrowUp) as i32;
        self.state.draw_size -= inputs.is_pressed(KeyCode::ArrowDown) as i32;
        self.state.draw_size = self.state.draw_size.clamp(1, MAX_DRAW_SIZE);

        // Cycle shape on Tab
        if inputs.is_pressed(KeyCode::Tab) {
            unsafe {
                let shape = transmute::<u8, Shape>((self.state.draw_shape as u8 + 1) % 3);
                self.state.draw_shape = shape;
            }
        }

        // velocity is bounded by equilibrium point with resistance
        // TODO(TOM): Change CAMERA_RESISTANCE to an easing function?
        self.camera_vel *= CAMERA_RESISTANCE;
        self.camera += self.camera_vel;
        self.state.mouse = inputs.mouse_pos;
    }

    fn handle_input_renders(&mut self, inputs: &mut InputData) {
        optick::event!("Handling Input Renders");

        if inputs.is_mouse_dragging() {
            Shape::draw_arrow(
                inputs.mouse_pressed.pos.scale(self.state.scale).cast(),
                inputs.mouse_pos.scale(self.state.scale).cast(),
                |x: i32, y: i32| {
                    let pos = vec2(x, y).clamp(vec2(0, 0), self.sim_size - 1);
                    self.write_to_buf(pos, RED);
                },
            );
        } else {
            self.clear_mouse_outline(GREEN);
            self.render_mouse_outline(GREEN);
        }
    }
    // endregion
    // region: Physics
    fn update_physics_cursor(&mut self, mouse: Vec2<f64, ScreenSpace>) {
        optick::event!("Physics Update - Cursor");
        let mouse = mouse.cast_unit();

        // All particles attract to mouse.
        self.particles
            .par_iter_mut()
            .map(|p| p.get_mut())
            .for_each(|p| {
                let dist = p.pos - mouse;
                let abs_dist = f64::sqrt(dist.x.pow(2) + dist.y.pow(2));

                if abs_dist > 5.0 {
                    // If collapsing in on cursor, give it some velocity.
                    let normal = p.pos.sub(mouse).mul(1.0 / abs_dist * PHYSICS_MULTIPLIER);
                    p.vel -= normal;
                } else {
                    // Branchless!
                    let mut delta = vec2(-1.0, -1.0);
                    let are_vels_neg = p.vel.map(|n| (n < 0.0) as i32 as f64);
                    delta += are_vels_neg * 2.0;
                    p.vel += delta;
                }
                p.vel *= PHYSICS_RESISTANCE;
                p.pos += p.vel;
            });
    }

    fn update_physics(&mut self) {
        optick::event!("Physics Update");

        for (i, p1) in self.particles.iter().enumerate() {
            let p1 = p1.get_mut();
            if p1.mass == 0.0 {
                continue;
            }

            for (j, p2) in self.particles.iter().enumerate().skip(i) {
                let p2 = p2.get_mut();
                if i == j || p2.mass == 0.0 {
                    continue;
                }

                // get distance between objects
                let dist = p2.pos.sub(p1.pos);
                let abs_dist = f64::sqrt(dist.x.pow(2) + dist.y.pow(2));

                if abs_dist < 0.95 * p1.radius.max(p2.radius) {
                    // collide entities
                    let consumer_pos = if p1.mass > p2.mass { p1.pos } else { p2.pos };
                    let new_mass = p1.mass + p2.mass;
                    let new_momentum = p1.vel * p1.mass + p2.vel * p2.mass;
                    let new_radius = f64::sqrt(p1.radius.pow(2) + p2.radius.pow(2));

                    *p1 = Particle {
                        pos: consumer_pos,
                        vel: new_momentum / new_mass,
                        mass: new_mass,
                        radius: new_radius,
                    };

                    // will be culled later.
                    *p2 = Particle {
                        pos: vec2(f64::MIN, f64::MIN), // TODO(TOM): MIN might cause slowdowns? prob not..
                        vel: vec2(0.0, 0.0),
                        mass: 0.0,
                        radius: 0.0,
                    };
                } else {
                    // calc physics
                    let p1_unit_vector = dist / abs_dist;

                    let abs_force = GRAV_CONST * (p1.mass * p2.mass) / abs_dist.pow(2.0);

                    let p1_force = p1_unit_vector * abs_force;
                    let p2_force = p1_force * -1.0; // Equal and opposite!

                    // println!(
                    //     "{p1_unit_vector:?} = {dist:?} / {abs_dist}\n\
                    //             {abs_force}\n\
                    //             {p1_force:?} || {p2_force:?}\n\
                    //             {vel:?} = {p2_force:?} / {}\n",
                    //     p2.mass,
                    //     vel = p2_force / p2.mass
                    // );
                    // TODO(TOM): dela time will break if sim looping slower than TARGET_FPS
                    p1.vel += p1_force / p1.mass * (1.0 / TARGET_FPS); // 1.0 / TARGET_FPS == delta time
                    p2.vel += p2_force / p2.mass * (1.0 / TARGET_FPS); // 1.0 / TARGET_FPS == delta time
                }
            }

            // apply resitance & update pos after all forces have been calculated.

            p1.vel *= PHYSICS_RESISTANCE;
            p1.pos += p1.vel;
        }

        // TODO(TOM): ideally cull particles in the same loop, mutability & iterator validity issues.
        self.particles
            .retain(|p| p.get().mass != 0.0 && p.get().radius != 0.0);
    }

    // endregion
    // region: Rendering
    fn render_particles(
        texture_buf: &[SyncCell<u8>],
        particles: &[SyncCell<Particle>],
        sim_size: Vec2<i32, RenderSpace>,
        camera: Vec2<f64, WorldSpace>,
    ) {
        optick::event!("Update Texture Buffer");

        particles
            .iter()
            .map(|p| p.get_mut())
            .map(|p| (p.pos.sub(camera), p.radius))
            .filter(|(pos, radius)| {
                !(pos.x + radius < 0.0
                    || pos.y + radius < 0.0
                    || pos.x - radius >= f64::from(sim_size.x)
                    || pos.y - radius >= f64::from(sim_size.y))
            })
            .for_each(|(pos, radius)| {
                Shape::CircleOutline.draw(radius as i32, |off_x, off_y| {
                    let offset = pos.map(|n| n as i32) + vec2(off_x, off_y);
                    if !(offset.x < 0
                        || offset.y < 0
                        || offset.x >= sim_size.x
                        || offset.y >= sim_size.y)
                    {
                        let index = 4 * (offset.y * sim_size.x + offset.x) as usize;
                        Self::write_colour(index, texture_buf, WHITE);
                    }
                });
            });
    }

    // TODO(TOM): make this a separate texture layer, overlayed on top of the sim
    fn render_mouse_outline(&mut self, colour: Rgba) {
        optick::event!("Rendering Mouse Outline");
        let mouse = self.state.mouse.scale(self.state.scale);

        self.state
            .draw_shape
            .draw(self.state.draw_size, |off_x, off_y| {
                // avoids u32 underflow
                let mut pos = mouse.cast::<i32>() + vec2(off_x, off_y);
                pos = pos.clamp(vec2(0, 0), self.sim_size - 1);

                self.write_to_buf(pos, colour);
            });
    }

    // TODO(TOM): this function proper doesn't work with back buffers
    fn clear_mouse_outline(&mut self, colour: Rgba) {
        optick::event!("Clearing Mouse Outline");
        let mouse = self.prev_state.mouse.scale(self.prev_state.scale);

        self.prev_state
            .draw_shape
            .draw(self.prev_state.draw_size, |off_x, off_y| {
                // avoids u32 underflow
                let mut pos = mouse.cast::<i32>() + vec2(off_x, off_y);
                pos = pos.clamp(vec2(0, 0), self.sim_size - 1);

                let index = 4 * (pos.y * self.sim_size.x + pos.x) as usize;
                let buf = &mut self.bufs[self.front_buffer];
                if *buf[index + 0].get_mut() == colour.r
                    && *buf[index + 1].get_mut() == colour.g
                    && *buf[index + 2].get_mut() == colour.b
                    && *buf[index + 3].get_mut() == colour.a
                {
                    Self::write_colour(index, buf, DGRAY);
                }
            });
    }
    // endregion

    pub fn new(window_size: Vec2<u32, ScreenSpace>, scale: u32) -> Self {
        let scale = Scale::new(scale as i32);
        let window_size = window_size.cast();

        let sim_size = window_size.scale(scale);
        let buf_size = (sim_size.x * sim_size.y * 4) as usize;

        let mut buf = Vec::with_capacity(buf_size);
        let mut buf_clone = Vec::with_capacity(buf_size);
        for _ in 0..buf_size {
            buf.push(SyncCell::new(44));
            buf_clone.push(SyncCell::new(44));
        }

        let mut particles = Vec::new();
        particles.extend_from_slice(&Self::init_particles());

        println!("particles: {particles:#?}");

        let state = State {
            frame: 0,
            draw_size: INIT_DRAW_SIZE,
            draw_shape: Shape::CircleFill,
            scale,
            running: false,
            step_sim: false,
            mouse: vec2(0.0, 0.0),
        };
        Self {
            state,
            prev_state: state,

            window_size,
            sim_size,
            camera: vec2(0.0, 0.0),
            camera_vel: vec2(0.0, 0.0),
            bufs: [buf, buf_clone],
            front_buffer: 0,
            particles,
        }
    }
}
