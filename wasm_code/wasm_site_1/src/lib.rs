#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{window, HtmlCanvasElement, CanvasRenderingContext2d};
use gloo_net::http::Request;
use gloo_timers::future::TimeoutFuture;
use js_sys::Math;
use std::cell::RefCell;
use std::rc::Rc;

thread_local! {
    static HOSTNAME: RefCell<String> = RefCell::new("connecting...".to_string());
    static CPU:      RefCell<f64>    = RefCell::new(0.0);
    static RAM_USED: RefCell<u64>    = RefCell::new(0);
    static RAM_TOTAL:RefCell<u64>    = RefCell::new(1);
    static UPTIME:   RefCell<u64>    = RefCell::new(0);
    static OS_INFO:  RefCell<String> = RefCell::new(String::new());
}

fn extract_str_field(json: &str, key: &str) -> Option<String> {
    let search = format!("\"{}\":\"", key);
    let start = json.find(&search)? + search.len();
    let end = start + json[start..].find('"')?;
    Some(json[start..end].to_string())
}

fn extract_num_field(json: &str, key: &str) -> Option<f64> {
    let search = format!("\"{}\":", key);
    let start = json.find(&search)? + search.len();
    let rest = &json[start..];
    let end = rest.find(|c: char| c == ',' || c == '}' || c == '\n')
        .unwrap_or(rest.len());
    rest[..end].trim().parse().ok()
}

fn update_state(json: &str) {
    if let Some(v) = extract_str_field(json, "hostname")          { HOSTNAME.with(|s| *s.borrow_mut() = v); }
    if let Some(v) = extract_num_field(json, "cpu_usage_percent") { CPU.with(|s| *s.borrow_mut() = v); }
    if let Some(v) = extract_num_field(json, "used_ram_bytes")    { RAM_USED.with(|s| *s.borrow_mut() = v as u64); }
    if let Some(v) = extract_num_field(json, "total_ram_bytes")   { RAM_TOTAL.with(|s| *s.borrow_mut() = v as u64); }
    if let Some(v) = extract_num_field(json, "uptime_seconds")    { UPTIME.with(|s| *s.borrow_mut() = v as u64); }
    if let Some(v) = extract_str_field(json, "os")                { OS_INFO.with(|s| *s.borrow_mut() = v); }
}

const CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789@#$%^&*";

struct Rain {
    cols: Vec<f64>,
    col_w: f64,
    char_list: Vec<char>,
}

impl Rain {
    fn new(w: f64, h: f64) -> Self {
        let col_w = 18.0;
        let n = (w / col_w) as usize + 2;
        let cols = (0..n).map(|_| Math::random() * -h).collect();
        let char_list = CHARS.chars().collect();
        Rain { cols, col_w, char_list }
    }

    fn tick(&mut self, ctx: &CanvasRenderingContext2d, w: f64, h: f64) {
        ctx.set_fill_style(&JsValue::from_str("rgba(0,0,0,0.04)"));
        ctx.fill_rect(0.0, 0.0, w, h);
        ctx.set_font("15px monospace");
        let n = self.char_list.len();
        for (i, y) in self.cols.iter_mut().enumerate() {
            let x = i as f64 * self.col_w;
            let idx = (Math::random() * n as f64) as usize % n;
            let ch = self.char_list[idx].to_string();
            ctx.set_fill_style(&JsValue::from_str("rgba(180,255,180,0.85)"));
            let _ = ctx.fill_text(&ch, x, *y);
            *y += 18.0;
            if *y > h && Math::random() > 0.975 {
                *y = Math::random() * -150.0;
            }
        }
    }
}

fn draw_bar(ctx: &CanvasRenderingContext2d, x: f64, y: f64, w: f64, pct: f64, color: &str) {
    ctx.set_fill_style(&JsValue::from_str("rgba(255,255,255,0.08)"));
    ctx.fill_rect(x, y, w, 10.0);
    ctx.set_fill_style(&JsValue::from_str(color));
    ctx.fill_rect(x, y, w * pct.clamp(0.0, 1.0), 10.0);
}

fn draw_overlay(ctx: &CanvasRenderingContext2d, w: f64, h: f64, frame: u32) {
    let hostname  = HOSTNAME.with(|s| s.borrow().clone());
    let cpu       = CPU.with(|s| *s.borrow());
    let ram_used  = RAM_USED.with(|s| *s.borrow());
    let ram_total = RAM_TOTAL.with(|s| *s.borrow());
    let uptime    = UPTIME.with(|s| *s.borrow());
    let os_info   = OS_INFO.with(|s| s.borrow().clone());

    let pulse = 0.70 + 0.30 * f64::sin(frame as f64 * 0.04);

    // Hostname box centered
    let bw = 560.0_f64;
    let bh = 95.0_f64;
    let bx = (w - bw) / 2.0;
    let by = h / 2.0 - bh / 2.0;
    ctx.set_fill_style(&JsValue::from_str("rgba(0,0,0,0.78)"));
    ctx.fill_rect(bx, by, bw, bh);
    ctx.set_stroke_style(&JsValue::from_str(&format!("rgba(0,255,65,{:.2})", pulse)));
    ctx.set_line_width(1.5);
    ctx.stroke_rect(bx, by, bw, bh);
    ctx.set_fill_style(&JsValue::from_str(&format!("rgba(0,255,65,{:.2})", pulse * 0.9)));
    ctx.set_font("11px monospace");
    let _ = ctx.fill_text("[ HOSTNAME ]", bx + 12.0, by + 18.0);
    ctx.set_fill_style(&JsValue::from_str(&format!("rgba(255,255,255,{:.2})", pulse)));
    ctx.set_font("bold 40px monospace");
    let _ = ctx.fill_text(&hostname, bx + 12.0, by + 72.0);

    // Stats panel bottom-left
    let px = 30.0;
    let py = h - 195.0;
    let pw = 310.0;
    ctx.set_fill_style(&JsValue::from_str("rgba(0,0,0,0.68)"));
    ctx.fill_rect(px - 12.0, py - 24.0, pw, 200.0);
    ctx.set_fill_style(&JsValue::from_str("#00ff41"));
    ctx.set_font("13px monospace");

    let _ = ctx.fill_text(&format!("CPU   {:.1}%", cpu), px, py);
    draw_bar(ctx, px, py + 6.0, pw - 36.0, cpu / 100.0, "#00ff41");

    let ram_pct = if ram_total > 0 { ram_used as f64 / ram_total as f64 } else { 0.0 };
    let ru = ram_used  as f64 / 1_073_741_824.0;
    let rt = ram_total as f64 / 1_073_741_824.0;
    let _ = ctx.fill_text(&format!("RAM   {:.1} / {:.1} GB", ru, rt), px, py + 38.0);
    draw_bar(ctx, px, py + 44.0, pw - 36.0, ram_pct, "#00aaff");

    let days = uptime / 86400;
    let hrs  = (uptime % 86400) / 3600;
    let mins = (uptime % 3600) / 60;
    let secs = uptime % 60;
    let _ = ctx.fill_text(&format!("UP    {}d {:02}h {:02}m {:02}s", days, hrs, mins, secs), px, py + 80.0);

    let os_short = if os_info.len() > 34 { &os_info[..34] } else { &os_info };
    let _ = ctx.fill_text(&format!("OS    {}", os_short), px, py + 108.0);

    // Clock top-right
    let now = js_sys::Date::new_0();
    let clock = format!("{:02}:{:02}:{:02}", now.get_hours(), now.get_minutes(), now.get_seconds());
    ctx.set_fill_style(&JsValue::from_str("rgba(0,0,0,0.68)"));
    ctx.fill_rect(w - 215.0, 18.0, 200.0, 48.0);
    ctx.set_fill_style(&JsValue::from_str("#00ff41"));
    ctx.set_font("bold 30px monospace");
    let _ = ctx.fill_text(&clock, w - 205.0, 54.0);
}

fn raf(f: &Closure<dyn FnMut()>) {
    window().unwrap().request_animation_frame(f.as_ref().unchecked_ref()).unwrap();
}

#[wasm_bindgen]
pub fn start(api_url: String) {
    let win = window().unwrap();
    let doc = win.document().unwrap();

    let canvas: HtmlCanvasElement = doc
        .get_element_by_id("canvas").unwrap()
        .dyn_into().unwrap();

    let w = win.inner_width().unwrap().as_f64().unwrap();
    let h = win.inner_height().unwrap().as_f64().unwrap();
    canvas.set_width(w as u32);
    canvas.set_height(h as u32);

    let ctx: CanvasRenderingContext2d = canvas
        .get_context("2d").unwrap().unwrap()
        .dyn_into().unwrap();
    ctx.set_fill_style(&JsValue::from_str("black"));
    ctx.fill_rect(0.0, 0.0, w, h);

    // Periodic sysinfo fetch
    spawn_local(async move {
        loop {
            if let Ok(resp) = Request::get(&api_url).send().await {
                if let Ok(text) = resp.text().await {
                    update_state(&text);
                }
            }
            TimeoutFuture::new(2000).await;
        }
    });

    // Animation loop
    let rain  = Rc::new(RefCell::new(Rain::new(w, h)));
    let frame = Rc::new(RefCell::new(0u32));
    let ctx   = Rc::new(ctx);

    let anim: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let anim_outer = anim.clone();

    *anim_outer.borrow_mut() = Some(Closure::new(move || {
        let f = {
            let mut v = frame.borrow_mut();
            *v = v.wrapping_add(1);
            *v
        };
        rain.borrow_mut().tick(&ctx, w, h);
        draw_overlay(&ctx, w, h, f);
        raf(anim.borrow().as_ref().unwrap());
    }));

    raf(anim_outer.borrow().as_ref().unwrap());
    Box::leak(Box::new(anim_outer));
}
