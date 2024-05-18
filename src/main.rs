extern crate winapi;

use std::f32::consts::PI;
use std::os::windows::ffi::OsStrExt;
use std::io::Error;
use std::sync::{Arc, Mutex};
use speedy2d::color::Color;
use speedy2d::dimen::{UVec2, Vec2};
use speedy2d::font::{Font, TextLayout, TextOptions};
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
    color: f32,
    subdir: Option<Vec<DirEntry>>,
}

const MAX_THREAD_COUNT: u32 = 32;

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
                                color: next_color_count(),
                                subdir: Some(subdir_scan.1)
                            };
                        }));
                    } else {
                        drop(thread_count);
                        let subdir_scan = scan_dir(&entry.path(), thread_count_mutex);
                        dir_entries_mutex.lock().unwrap()[i] = DirEntry {
                            name: file_name,
                            size: subdir_scan.0,
                            color: next_color_count(),
                            subdir: Some(subdir_scan.1)
                        };
                    }
                } else {
                    file_size = get_disk_size(entry.path()).unwrap_or_else(|_| entry.metadata().unwrap().file_size());
                    dir_entries_mutex.lock().unwrap()[i] = DirEntry {
                        name: file_name,
                        size: file_size,
                        color: next_color_count(),
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

fn color_mutex() -> &'static Mutex<f32> {
    static COLOR_COUNT: Mutex<f32> = Mutex::new(0.0);
    &COLOR_COUNT
}


fn next_color_count() -> f32 {
    let mut color_count_ref = color_mutex().lock().unwrap();
    let color_count = *color_count_ref;
    *color_count_ref += 1.0;
    color_count
}

fn reset_color_count() {
    *color_mutex().lock().unwrap() = 0.0;
}



fn draw_dir_entry(graphics: &mut Graphics2D, dir_entry: &DirEntry, wh: &MyWindowHandler, distance: u32, start_angle: f32, end_angle: f32, enable_recursion: bool) {
    if wh.cull_min_angle > wh.cull_max_angle {
        if start_angle > wh.cull_max_angle && end_angle < wh.cull_min_angle { return }
    } else {
        if start_angle > wh.cull_max_angle || end_angle < wh.cull_min_angle { return }
    }
    
    let radius = match enable_recursion && dir_entry.subdir.is_some() {
        true => N - N * f32::powi((N-1.0) / N, distance as i32),
        false => N
    };
    
    if enable_recursion && radius < wh.cull_max_radius {
        if let Some(subdir_entries) = &dir_entry.subdir {
            let mut angle = start_angle;
            let mut angle_delta_carry = 0.0;
            let mut subdir_entry_carry = None;
            for subdir_entry in subdir_entries {
                if subdir_entry.size == 0 { continue }
                
                let angle_delta = subdir_entry.size as f32 / dir_entry.size as f32 * (end_angle - start_angle);
                if angle_delta * wh.scale * N >= 1.0 {
                    if let Some(subdir_entry_past) = subdir_entry_carry {
                        draw_dir_entry(graphics, subdir_entry_past, wh, distance + 1, angle, angle + angle_delta_carry, false);
                        angle += angle_delta_carry;
                        angle_delta_carry = 0.0;
                        subdir_entry_carry = None;
                    }
                    draw_dir_entry(graphics, &subdir_entry, wh, distance + 1, angle, angle + angle_delta, true);
                    angle += angle_delta;
                } else {
                    angle_delta_carry += angle_delta;
                    if subdir_entry_carry.is_none() {
                        subdir_entry_carry = Some(subdir_entry);
                    }
                    if angle_delta_carry * wh.scale * N >= 1.0 {
                        draw_dir_entry(graphics, subdir_entry_carry.unwrap_or(subdir_entry), wh, distance + 1, angle, angle + angle_delta_carry, false);
                        angle += angle_delta_carry;
                        angle_delta_carry = 0.0;
                        subdir_entry_carry = None;
                    }
                }
            }
        }
    }
    
    
    let mut points = vec![(0.0, 0.0)];
    let mut angle = start_angle;
    while angle < end_angle {
        points.push((wh.scale * radius * f32::cos(angle), wh.scale * radius * f32::sin(angle)));
        angle += INCREMENT;
    }
    points.push((wh.scale * radius * f32::cos(end_angle), wh.scale * radius * f32::sin(end_angle)));
    
    graphics.draw_polygon(&Polygon::new(&points), wh.center_pos, from_hsv(0.65 + 0.04 * (distance as f32 + wh.current_dir_path.len() as f32), 0.7, (dir_entry.color * PI) % 0.7 + 0.3));
    
    if dir_entry.subdir.is_some() {
        let thickness = 0.1 * wh.scale / distance as f32;
        let mut angle = start_angle;
        while angle + INCREMENT < end_angle {
            graphics.draw_line(
                wh.center_pos + Vec2::new(angle.cos(), angle.sin()) * wh.scale * radius,
                wh.center_pos + Vec2::new((angle + INCREMENT).cos(), (angle + INCREMENT).sin()) * wh.scale * radius,
            thickness, Color::BLACK);
            angle += INCREMENT;
        }
        graphics.draw_line(
            wh.center_pos + Vec2::new(angle.cos(), angle.sin()) * wh.scale * radius,
            wh.center_pos + Vec2::new(end_angle.cos(), end_angle.sin()) * wh.scale * radius,
        thickness, Color::BLACK);
    }
    
    if dir_entry.subdir.is_some() && !(start_angle == 0.0 && end_angle == 2.0*PI) {
        let thickness = (0.2 * (end_angle - start_angle) * wh.scale * N).clamp(0.0, 4.0);
        graphics.draw_line(
            wh.center_pos,
            wh.center_pos + Vec2::new(start_angle.cos(), start_angle.sin()) * wh.scale * N,
        thickness, Color::BLACK);
        graphics.draw_line(
            wh.center_pos,
            wh.center_pos + Vec2::new(end_angle.cos(), end_angle.sin()) * wh.scale * N,
        thickness, Color::BLACK);
    }
}



struct MyWindowHandler {
    root: DirEntry,
    font: Font,
    current_dir_path: Vec<usize>,
    center_pos: Vec2,
    scale: f32,
    mouse_left: bool,
    mouse_middle: bool,
    mouse_right: bool,
    mouse_pos: Vec2,
    window_size: UVec2,
    cull_max_radius: f32,
    cull_min_angle: f32,
    cull_max_angle: f32,
}

impl MyWindowHandler {
    fn update_view(&mut self) {
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
        
        
        let corners = [
            Vec2::new(0.0, 0.0),
            Vec2::new(self.window_size.x as f32, 0.0),
            Vec2::new(0.0, self.window_size.y as f32),
            Vec2::new(self.window_size.x as f32, self.window_size.y as f32),
        ];
        
        self.cull_max_radius = 0.0;
        self.cull_min_angle = 0.0;
        self.cull_max_angle = 2.0*PI;
        for corner in corners {
            let radius = (corner - self.center_pos).magnitude() / self.scale;
            if self.cull_max_radius < radius { self.cull_max_radius = radius }
        }
        
        if self.center_pos.x >= 0.0 && self.center_pos.x <= self.window_size.x as f32 && 
           self.center_pos.y >= 0.0 && self.center_pos.y <= self.window_size.y as f32 {
            self.cull_min_angle = 0.0;
            self.cull_max_angle = 2.0*PI;
        } else {
            let mut angles = vec![];
            for corner in corners {
                let a = f32::atan2(corner.y - self.center_pos.y, corner.x - self.center_pos.x);
                angles.push(if a < 0.0 { a + 2.0*PI } else { a });
            }
            angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
            if angles[3] > angles[0] + PI {
                self.cull_min_angle = angles[2];
                self.cull_max_angle = angles[1];
            } else {
                self.cull_min_angle = angles[0];
                self.cull_max_angle = angles[3];
            }
        }
    }
    
    fn find_file(&self, dir_entry: &DirEntry, select_angle: f32, select_radius: f32, distance: u32, start_angle: f32, end_angle: f32) -> Vec<usize> {
        let radius = match dir_entry.subdir.is_some() {
            true => N - N * f32::powi((N-1.0) / N, distance as i32),
            false => N
        };
        
        if select_radius < radius {
            return vec![]
        }
        
        if let Some(subdir_entries) = &dir_entry.subdir {
            let mut angle = start_angle;
            for i in 0..subdir_entries.len() {
                let angle_delta = subdir_entries[i].size as f32 / dir_entry.size as f32 * (end_angle - start_angle);
                if angle + angle_delta > select_angle {
                    let mut v = self.find_file(&subdir_entries[i], select_angle, select_radius, distance + 1, angle, angle + angle_delta);
                    v.push(i);
                    return v
                }
                angle += angle_delta;
            }
        }
        
        return vec![]
    }
}

impl WindowHandler for MyWindowHandler {
    fn on_mouse_button_down(&mut self, _helper: &mut WindowHelper<()>, button: MouseButton) {
        match button {
            MouseButton::Left => {
                self.mouse_left = true;
                
                let mouse_angle = f32::atan2(self.mouse_pos.y - self.center_pos.y, self.mouse_pos.x - self.center_pos.x);
                let mouse_angle = if mouse_angle < 0.0 { mouse_angle + 2.0*PI } else { mouse_angle };
                let mouse_radius = (self.mouse_pos - self.center_pos).magnitude() / self.scale;
                
                if mouse_radius <= N {
                    let mut current_node = &self.root;
                    for index in &self.current_dir_path {
                        if let Some(subdir) = &current_node.subdir {
                            current_node = &subdir[*index];
                        } else {
                            break;
                        }
                    }
                    
                    let mut index_path = self.find_file(current_node, mouse_angle, mouse_radius, 1, 0.0, 2.0*PI);
                    if index_path.len() == 0 {
                        self.current_dir_path.pop();
                    } else {
                        index_path.reverse();
                        self.current_dir_path.append(&mut index_path);
                    }
                    
                }
            }
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
            self.update_view();
        }
        
        self.mouse_pos = position;
    }
    
    fn on_mouse_wheel_scroll(&mut self, _helper: &mut WindowHelper<()>, distance: MouseScrollDistance) {
        if let MouseScrollDistance::Lines { y: delta, x: _, z: _ } = distance {
            let ratio = 1.0 + 0.1 * delta as f32;
            self.scale *= ratio;
            self.center_pos = self.mouse_pos + (self.center_pos - self.mouse_pos) * ratio;
            self.update_view();
        }
    }
    
    fn on_resize(&mut self, _helper: &mut WindowHelper<()>, size_pixels: UVec2) {
        self.scale *= size_pixels.y as f32 / self.window_size.y as f32;
        self.center_pos.x += (size_pixels.x as f32 - self.window_size.x as f32) / 2.0;
        self.center_pos.y += (size_pixels.y as f32 - self.window_size.y as f32) / 2.0;
        self.window_size = size_pixels;
        self.update_view();
    }
    
    
    fn on_draw(&mut self, helper: &mut WindowHelper<()>, graphics: &mut Graphics2D) {
        
        let mut current_dir_name = self.root.name.clone();
        
        let mut current_node = &self.root;
        for index in &self.current_dir_path {
            if let Some(subdir) = &current_node.subdir {
                current_node = &subdir[*index];
                current_dir_name = current_dir_name + "\\" + &current_node.name;
            } else {
                break;
            }
        }
        
        graphics.clear_screen(Color::DARK_GRAY);
        reset_color_count();
        
        draw_dir_entry(graphics, current_node, self, 1, 0.0, 2.0*PI, true);
        
        for angle in 0..360 {
            let angle = angle as f32 * PI/180.0;
            graphics.draw_line(
                self.center_pos + Vec2::new(angle.cos(), angle.sin()) * self.scale * N,
                self.center_pos + Vec2::new((angle + INCREMENT).cos(), (angle + INCREMENT).sin()) * self.scale * N,
            0.05 * self.scale, Color::BLACK);
        }
        
        let mouse_angle = f32::atan2(self.mouse_pos.y - self.center_pos.y, self.mouse_pos.x - self.center_pos.x);
        let mouse_angle = if mouse_angle < 0.0 { mouse_angle + 2.0*PI } else { mouse_angle };
        let mouse_radius = (self.mouse_pos - self.center_pos).magnitude() / self.scale;
        
        let mut file_name;
        let mut node = current_node;
        
        if mouse_radius <= N {
            let index_path = self.find_file(current_node, mouse_angle, mouse_radius, 1, 0.0, 2.0*PI);
            
            if index_path.len() == 0 {
                file_name = current_dir_name;
            } else {
                file_name = current_node.name.clone();
                for index in index_path.iter().rev() {
                    if let Some(subdir) = &node.subdir {
                        node = &subdir[*index];
                        file_name = file_name + "\\" + &node.name;
                    } else {
                        break
                    }
                }
            }
        } else {
            file_name = current_dir_name;
        }
        
        const METRIC_PREFIXES: [&str; 8] = ["", "K", "M", "G", "T", "P", "E", "Y"];
        
        let mut bytes = node.size as f32;
        let mut prefix_index = 0;
        while bytes >= 1024.0 {
            bytes /= 1024.0;
            prefix_index += 1;
        }
        
        graphics.draw_text((12.0, self.window_size.y as f32 - 72.0), Color::WHITE, &self.font.layout_text(
            &(bytes.to_string().get(..5).unwrap_or(&bytes.to_string()).to_owned() + " " + METRIC_PREFIXES[prefix_index] + "B"),
        30.0, TextOptions::new()));
        graphics.draw_text((12.0, self.window_size.y as f32 - 36.0), Color::WHITE, &self.font.layout_text(&file_name, 30.0, TextOptions::new()));
        
        helper.request_redraw();
    }
}





fn main() {
    let window_size = UVec2::new(800, 800);
    let window = Window::new_centered("Disk Pie", window_size).unwrap();
    
    let root_folder = "C:\\";
    
    let mut window_handler = MyWindowHandler {
        root: {
            let (size, dirs) = scan_dir(&std::path::PathBuf::from(root_folder), &Arc::new(Mutex::new(1)));
            DirEntry {
                name: String::from(root_folder.strip_suffix("\\").unwrap_or(&root_folder)),
                size,
                color: next_color_count(),
                subdir: Some(dirs)
            }
        },
        font: Font::new(include_bytes!("OpenSans-Regular.ttf")).unwrap(),
        current_dir_path: vec![],
        center_pos: Vec2::new(window_size.x as f32 / 2.0, window_size.y as f32 / 2.0),
        scale: window_size.y as f32 / 12.0,
        mouse_left: false,
        mouse_middle: false,
        mouse_right: false,
        mouse_pos: Vec2::new(window_size.x as f32 / 2.0, window_size.y as f32 / 2.0),
        window_size,
        cull_max_radius: 0.0,
        cull_min_angle: 0.0,
        cull_max_angle: 2.0*PI,
    };
    
    window_handler.update_view();
    
    window.run_loop(window_handler);
}
