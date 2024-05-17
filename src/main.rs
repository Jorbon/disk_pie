extern crate winapi;

use std::f32::consts::PI;
use std::os::windows::ffi::OsStrExt;
use std::io::Error;
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



struct DirEntry(String, u64, Option<Vec<DirEntry>>);


use std::os::windows::fs::MetadataExt;

fn scan_dir(path: &std::path::PathBuf) -> (Vec<DirEntry>, u64) {
    match std::fs::read_dir(path) {
        Ok(dir) => {
            let mut size = 0;
            
            let dir_entries = dir.map(|entry| {
                let entry = entry.unwrap();
                if entry.metadata().unwrap().is_dir() {
                    let (subdir_entries, subdir_size) = scan_dir(&entry.path());
                    size += subdir_size;
                    DirEntry(entry.file_name().into_string().unwrap(), subdir_size, Some(subdir_entries))
                } else {
                    let file_size = get_disk_size(entry.path()).unwrap_or_else(|_| entry.metadata().unwrap().file_size());
                    size += file_size;
                    DirEntry(entry.file_name().into_string().unwrap(), file_size, None)
                }
            }).collect();
            
            (dir_entries, size)
        }
        Err(e) => {
            println!("{e} : {}", path.display());
            (vec![], 0)
        }
    }
}



const COLOR_TABLE: [Color; 9] = [
    Color::CYAN,
    Color::YELLOW,
    Color::RED,
    Color::GRAY,
    Color::GREEN,
    Color::BLUE,
    Color::LIGHT_GRAY,
    Color::MAGENTA,
    Color::DARK_GRAY,
];

static mut NEXT_COLOR_INDEX: usize = COLOR_TABLE.len() - 1;

fn reset_color() {
    unsafe {
        NEXT_COLOR_INDEX = COLOR_TABLE.len() - 1;
    }
}

fn next_color() -> Color {
    unsafe {
        NEXT_COLOR_INDEX += 1;
        if NEXT_COLOR_INDEX >= COLOR_TABLE.len() {
            NEXT_COLOR_INDEX = 0;
        }
        COLOR_TABLE[NEXT_COLOR_INDEX]
    }
}



struct MyWindowHandler {
    drive: (Vec<DirEntry>, u64),
    center_pos: Vec2,
    scale: f32,
    mouse_left: bool,
    mouse_middle: bool,
    mouse_right: bool,
    mouse_pos: Vec2,
    window_size: UVec2,
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
        }
        
        self.mouse_pos = position;
    }
    
    fn on_mouse_wheel_scroll(&mut self, _helper: &mut WindowHelper<()>, distance: MouseScrollDistance) {
        if let MouseScrollDistance::Lines { y: delta, x: _, z: _ } = distance {
            let ratio = 1.0 + 0.1 * delta as f32;
            self.scale *= ratio;
            self.center_pos = self.mouse_pos + (self.center_pos - self.mouse_pos) * ratio;
        }
    }
    
    fn on_resize(&mut self, _helper: &mut WindowHelper<()>, size_pixels: UVec2) {
        self.scale *= size_pixels.y as f32 / self.window_size.y as f32;
        self.center_pos.x += (size_pixels.x as f32 - self.window_size.x as f32) / 2.0;
        self.center_pos.y += (size_pixels.y as f32 - self.window_size.y as f32) / 2.0;
        
        self.window_size = size_pixels;
    }
    
    
    fn on_draw(&mut self, helper: &mut WindowHelper<()>, graphics: &mut Graphics2D) {
        graphics.clear_screen(Color::BLACK);
        reset_color();
        
        let mut progress = 0.0;
        for dir_entry in &self.drive.0 {
            let next_progress = progress + dir_entry.1 as f32 / self.drive.1 as f32 * 2.0*PI;
            const INCREMENT: f32 = 2.0*PI / 360.0;
            
            let mut points = vec![(0.0, 0.0)];
            while progress < next_progress {
                points.push((self.scale * f32::cos(progress), self.scale * f32::sin(progress)));
                progress += INCREMENT;
            }
            progress = next_progress;
            points.push((self.scale * f32::cos(progress), self.scale * f32::sin(progress)));
            
            graphics.draw_polygon(&Polygon::new(&points), (self.center_pos.x, self.center_pos.y), next_color());
        }
        
        helper.request_redraw();
    }
}





fn main() {
    let window_size = UVec2::new(1000, 1000);
    let window = Window::new_centered("Disk Pie", window_size).unwrap();
    
    window.run_loop(MyWindowHandler {
        drive: scan_dir(&std::path::PathBuf::from("C:\\Users\\benap\\OneDrive\\big\\RWBY")),
        center_pos: Vec2::new(window_size.x as f32 / 2.0, window_size.y as f32 / 2.0),
        scale: window_size.y as f32 / 12.0,
        mouse_left: false,
        mouse_middle: false,
        mouse_right: false,
        mouse_pos: Vec2::new(window_size.x as f32 / 2.0, window_size.y as f32 / 2.0),
        window_size,
    });
}
