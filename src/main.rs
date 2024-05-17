extern crate winapi;

use std::f32::consts::PI;
use std::os::windows::ffi::OsStrExt;
use std::io::Error;
use std::sync::{Arc, Mutex};
use speedy2d::color::Color;
use speedy2d::dimen::{UVec2, Vec2};
use speedy2d::shape::Polygon;
use speedy2d::window::{MouseButton, MouseScrollDistance, WindowHandler, WindowHelper};
use speedy2d::{Graphics2D, Window};
use winapi::um::fileapi::GetCompressedFileSizeW;

fn get_disk_size<P: AsRef<std::path::Path>>(path: P) -> Result<u64, Error> {
    let mut long_path: Vec<u16> = r"\\?\".encode_utf16().collect();
    long_path.extend(path.as_ref().as_os_str().encode_wide());
    long_path.push(0);
    let mut high: u32 = 0;
    let low = unsafe { GetCompressedFileSizeW(long_path.as_ptr(), &mut high) };
    if low == u32::MAX {
        Err(Error::last_os_error())
    } else {
        Ok(((high as u64) << 32) | low as u64)
    }
}



#[derive(Clone, Default)]
struct DirEntry {
    name: String,
    size: u64,
    subdir: Option<Vec<DirEntry>>,
}

const MAX_THREAD_COUNT: u32 = 16;

use std::os::windows::fs::MetadataExt;

fn scan_dir(path: &std::path::PathBuf, thread_count_mutex: &Arc<Mutex<u32>>) -> (u64, Vec<DirEntry>) {
    match std::fs::read_dir(path) {
        Ok(dir) => {
            let dir = dir.map(|entry| entry.unwrap()).collect::<Vec<_>>();
            
            let mut threads = vec![];
            let dir_entries_mutex = &Arc::new(Mutex::new(vec![DirEntry::default(); dir.len()]));
            
            for i in 0..dir.len() {
                let entry = &dir[i];
                let file_name = entry.file_name().into_string().unwrap();
                let file_size;
                
                if entry.metadata().unwrap().is_dir() {
                    let mut thread_count = thread_count_mutex.lock().unwrap();
                    if *thread_count < MAX_THREAD_COUNT {
                        *thread_count += 1;
                        drop(thread_count);
                        
                        let path = entry.path();
                        let thread_count_mutex_share = Arc::clone(thread_count_mutex);
                        let dir_entries_mutex_share = Arc::clone(dir_entries_mutex);
                        threads.push(std::thread::spawn(move || {
                            let subdir_scan = scan_dir(&path, &thread_count_mutex_share);
                            dir_entries_mutex_share.lock().unwrap()[i] = DirEntry {
                                name: file_name,
                                size: subdir_scan.0,
                                subdir: Some(subdir_scan.1)
                            };
                        }));
                    } else {
                        drop(thread_count);
                        let subdir_scan = scan_dir(&entry.path(), thread_count_mutex);
                        dir_entries_mutex.lock().unwrap()[i] = DirEntry {
                            name: file_name,
                            size: subdir_scan.0,
                            subdir: Some(subdir_scan.1)
                        };
                    }
                } else {
                    file_size = get_disk_size(entry.path()).unwrap_or_else(|_| entry.metadata().unwrap().file_size());
                    dir_entries_mutex.lock().unwrap()[i] = DirEntry {
                        name: file_name,
                        size: file_size,
                        subdir: None
                    };
                }
            }
            
            for thread in threads {
                thread.join().unwrap();
                *thread_count_mutex.lock().unwrap() -= 1;
            }
            
            let dir_entries = (*dir_entries_mutex.lock().unwrap()).clone();
            
            let mut size = 0;
            for dir_entry in dir_entries.iter() {
                size += dir_entry.size;
            }
            
            (size, dir_entries)
        }
        Err(e) => {
            println!("{e} : {}", path.display());
            (0, vec![])
        }
    }
}




fn from_hsv(mut h: f32, s: f32, v: f32) -> Color {
    let max = v;
    let c = s*v;
    let min = max - c;
    h = (h % 1.0) * 6.0;
         if h < 1.0 { Color::from_rgb(max, min + h*c, min) }
    else if h < 2.0 { Color::from_rgb(min + (2.0 - h)*c, max, min) }
    else if h < 3.0 { Color::from_rgb(min, max, min + (h - 2.0)*c) }
    else if h < 4.0 { Color::from_rgb(min, min + (4.0 - h)*c, max) }
    else if h < 5.0 { Color::from_rgb(min + (h - 4.0)*c, min, max) }
    else            { Color::from_rgb(max, min, min + (6.0 - h)*c) }
}



const N: f32 = 5.0;

const INCREMENT: f32 = 2.0*PI / 360.0;

static mut COLOR_COUNT: u32 = 0;

fn next_color_count() -> f32 {
    unsafe {
        COLOR_COUNT += 1;
        COLOR_COUNT as f32
    }
}

fn reset_color_count() {
    unsafe {
        COLOR_COUNT = 0;
    }
}



fn draw_dir_entry(graphics: &mut Graphics2D, dir_entry: &DirEntry, scale: f32, distance: u32, center_pos: Vec2, start_angle: f32, end_angle: f32, dir_borders: bool) {
    let radius = match dir_entry.subdir {
        Some(_) => N - N * f32::powi((N-1.0) / N, distance as i32),
        None => N
    };
    
    if let Some(subdir_entries) = &dir_entry.subdir {
        let mut angle = start_angle;
        for subdir_entry in subdir_entries {
            let next_angle = angle + subdir_entry.size as f32 / dir_entry.size as f32 * (end_angle - start_angle);
            draw_dir_entry(graphics, &subdir_entry, scale, distance + 1, center_pos, angle, next_angle, true);
            angle = next_angle;
        }
    }
    
    
    let mut points = vec![(0.0, 0.0)];
    let mut angle = start_angle;
    while angle < end_angle {
        points.push((scale * radius * f32::cos(angle), scale * radius * f32::sin(angle)));
        angle += INCREMENT;
    }
    points.push((scale * radius * f32::cos(end_angle), scale * radius * f32::sin(end_angle)));
    
    graphics.draw_polygon(&Polygon::new(&points), center_pos, from_hsv(0.65 + 0.04 * distance as f32, 0.7, (next_color_count() * PI) % 0.7 + 0.3));
    
    if dir_entry.subdir.is_some() {
        let mut angle = start_angle;
        while angle + INCREMENT < end_angle {
            graphics.draw_line(
                center_pos + Vec2::new(angle.cos(), angle.sin()) * scale * radius,
                center_pos + Vec2::new((angle + INCREMENT).cos(), (angle + INCREMENT).sin()) * scale * radius,
            0.1 * scale / distance as f32, Color::BLACK);
            angle += INCREMENT;
        }
        graphics.draw_line(
            center_pos + Vec2::new(angle.cos(), angle.sin()) * scale * radius,
            center_pos + Vec2::new(end_angle.cos(), end_angle.sin()) * scale * radius,
        0.1 * scale / distance as f32, Color::BLACK);
    }
    
    if dir_borders && dir_entry.subdir.is_some() {
        graphics.draw_line(center_pos,
            center_pos + Vec2::new(start_angle.cos(), start_angle.sin()) * scale * N,
        (0.02 * scale / distance as f32).clamp(1.0, 4.0), Color::BLACK);
        graphics.draw_line(center_pos,
            center_pos + Vec2::new(end_angle.cos(), end_angle.sin()) * scale * N,
        (0.02 * scale / distance as f32).clamp(1.0, 4.0), Color::BLACK);
    }
}



struct MyWindowHandler {
    root: DirEntry,
    center_pos: Vec2,
    scale: f32,
    mouse_left: bool,
    mouse_middle: bool,
    mouse_right: bool,
    mouse_pos: Vec2,
    window_size: UVec2,
}

impl MyWindowHandler {
    fn bound_view(&mut self) {
        let min_scale = u32::min(self.window_size.x, self.window_size.y) as f32 / (2.0 * (N + 1.0));
        if self.scale < min_scale {
            self.scale = min_scale;
        }
        
        let (left, right, top, bottom) = if self.window_size.x > self.window_size.y {
            (
                (self.window_size.x as f32 - self.window_size.y as f32) / 2.0,
                (self.window_size.x as f32 + self.window_size.y as f32) / 2.0,
                0.0,
                self.window_size.y as f32,
            )
        } else {
            (
                0.0,
                self.window_size.x as f32,
                (self.window_size.y as f32 - self.window_size.x as f32) / 2.0,
                (self.window_size.y as f32 + self.window_size.x as f32) / 2.0,
            )
        };
        
        let center_pos_x_max = left + self.scale * (N + 1.0);
        if self.center_pos.x > center_pos_x_max {
            self.center_pos.x = center_pos_x_max;
        }
        let center_pos_x_min = right - self.scale * (N + 1.0);
        if self.center_pos.x < center_pos_x_min {
            self.center_pos.x = center_pos_x_min;
        }
        
        let center_pos_y_max = top + self.scale * (N + 1.0);
        if self.center_pos.y > center_pos_y_max {
            self.center_pos.y = center_pos_y_max;
        }
        let center_pos_y_min = bottom - self.scale * (N + 1.0);
        if self.center_pos.y < center_pos_y_min {
            self.center_pos.y = center_pos_y_min;
        }
    }
}

impl WindowHandler for MyWindowHandler {
    fn on_mouse_button_down(&mut self, _helper: &mut WindowHelper<()>, button: MouseButton) {
        match button {
            MouseButton::Left => self.mouse_left = true,
            MouseButton::Middle => self.mouse_middle = true,
            MouseButton::Right => self.mouse_right = true,
            MouseButton::Other(_) => ()
        }
    }
    fn on_mouse_button_up(&mut self, _helper: &mut WindowHelper<()>, button: MouseButton) {
        match button {
            MouseButton::Left => self.mouse_left = false,
            MouseButton::Middle => self.mouse_middle = false,
            MouseButton::Right => self.mouse_right = false,
            MouseButton::Other(_) => ()
        }
    }
    fn on_mouse_move(&mut self, _helper: &mut WindowHelper<()>, position: Vec2) {
        if self.mouse_left {
            self.center_pos += position - self.mouse_pos;
            self.bound_view();
        }
        
        self.mouse_pos = position;
    }
    
    fn on_mouse_wheel_scroll(&mut self, _helper: &mut WindowHelper<()>, distance: MouseScrollDistance) {
        if let MouseScrollDistance::Lines { y: delta, x: _, z: _ } = distance {
            let ratio = 1.0 + 0.1 * delta as f32;
            self.scale *= ratio;
            self.center_pos = self.mouse_pos + (self.center_pos - self.mouse_pos) * ratio;
            self.bound_view();
        }
    }
    
    fn on_resize(&mut self, _helper: &mut WindowHelper<()>, size_pixels: UVec2) {
        self.scale *= size_pixels.y as f32 / self.window_size.y as f32;
        self.center_pos.x += (size_pixels.x as f32 - self.window_size.x as f32) / 2.0;
        self.center_pos.y += (size_pixels.y as f32 - self.window_size.y as f32) / 2.0;
        self.window_size = size_pixels;
        self.bound_view();
    }
    
    
    fn on_draw(&mut self, helper: &mut WindowHelper<()>, graphics: &mut Graphics2D) {
        graphics.clear_screen(Color::DARK_GRAY);
        reset_color_count();
        
        draw_dir_entry(graphics, &self.root, self.scale, 1, self.center_pos, 0.0, 2.0*PI, false);
        
        for angle in 0..360 {
            let angle = angle as f32 * PI/180.0;
            graphics.draw_line(
                self.center_pos + Vec2::new(angle.cos(), angle.sin()) * self.scale * N,
                self.center_pos + Vec2::new((angle + INCREMENT).cos(), (angle + INCREMENT).sin()) * self.scale * N,
            0.05 * self.scale, Color::BLACK);
        }
        
        helper.request_redraw();
    }
}





fn main() {
    let window_size = UVec2::new(1000, 1000);
    let window = Window::new_centered("Disk Pie", window_size).unwrap();
    
    let root_folder = "C:\\Users\\benap";
    
    window.run_loop(MyWindowHandler {
        root: {
            let (size, dirs) = scan_dir(&std::path::PathBuf::from(root_folder), &Arc::new(Mutex::new(1)));
            DirEntry {
                name: String::from(root_folder),
                size,
                subdir: Some(dirs)
            }
        },
        center_pos: Vec2::new(window_size.x as f32 / 2.0, window_size.y as f32 / 2.0),
        scale: window_size.y as f32 / 12.0,
        mouse_left: false,
        mouse_middle: false,
        mouse_right: false,
        mouse_pos: Vec2::new(window_size.x as f32 / 2.0, window_size.y as f32 / 2.0),
        window_size,
    });
}
