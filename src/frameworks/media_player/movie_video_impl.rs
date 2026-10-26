/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// Showing the video of a movie. The movie's file is read, its video track
// is decoded with crate::video and the frames are shown in a window that
// covers the screen, as the real movie player does. If anything goes wrong
// the movie just plays without a picture.
//
// The movie's window doesn't react to touches: they go to the app, which is
// what lets it skip the movie (by stopping the player), as it does on a real
// device.
//
// This file is included by movie_player.rs, which it shares its scope with.

use crate::frameworks::core_graphics::cg_image;
use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::image::Image;
use crate::video::mp4::VideoTrack;
use crate::video::mpeg4::Decoder;
use crate::window::DeviceOrientation;

/// How many frames to decode at most each time the run loop comes around, so
/// that a slow moment can't make the emulator stall.
const MAX_FRAMES_PER_STEP: u32 = 4;

/// The video of a movie while it is being shown.
struct VideoPlayback {
    /// The whole mp4 file.
    file: Vec<u8>,
    track: VideoTrack,
    decoder: Decoder,
    /// Index of the next frame to decode.
    next_sample: usize,
    /// When the movie started, which is what the frames are timed against.
    started: Instant,
    /// `UIWindow*` that covers the screen while the movie plays.
    window: id,
    /// `UIImageView*` showing the current frame.
    image_view: id,
    /// Whether a frame has been shown yet.
    shown_any: bool,
    /// Whether decoding has failed, after which the picture stays as it is.
    failed: bool,
}

/// Find the largest rectangle with the proportions of width x height that
/// fits in the middle of `bounds`.
fn fit_rect(bounds: &CGRect, width: f32, height: f32) -> CGRect {
    let scale = (bounds.size.width / width).min(bounds.size.height / height);
    let (w, h) = (width * scale, height * scale);
    CGRect {
        origin: CGPoint {
            x: bounds.origin.x + (bounds.size.width - w) / 2.0,
            y: bounds.origin.y + (bounds.size.height - h) / 2.0,
        },
        size: CGSize {
            width: w,
            height: h,
        },
    }
}

/// Start showing the video of the movie at the player's content URL, if it is
/// something that can be decoded. Returns the length of the video in
/// seconds.
fn start_video(env: &mut Environment, player: id) -> Option<NSTimeInterval> {
    let host_object = env.objc.borrow::<MPMoviePlayerControllerHostObject>(player);
    if let Some(video) = &host_object.video {
        // `play` was called again on a player that is already showing video.
        return Some(video.track.duration_seconds());
    }
    let content_url = host_object.content_url;
    if content_url == nil {
        return None;
    }

    let path = ns_url::to_rust_path(env, content_url);
    let Ok(file) = env.fs.read(&path) else {
        log!("Couldn't read the movie file {:?}", path);
        return None;
    };
    let track = match VideoTrack::parse(&file) {
        Ok(track) => track,
        Err(error) => {
            log!("Couldn't find the video in {:?}: {}", path, error);
            return None;
        }
    };
    let duration = track.duration_seconds();
    if track.codec != *b"mp4v" || track.width == 0 || track.height == 0 || duration <= 0.0 {
        log!(
            "The video of {:?} can't be shown (codec {:?}, {}x{})",
            path,
            String::from_utf8_lossy(&track.codec),
            track.width,
            track.height,
        );
        return None;
    }
    let mut decoder = Decoder::new();
    if let Err(error) = decoder.decode_packet(&track.config) {
        log!("The video of {:?} can't be shown: {:?}", path, error);
        return None;
    }

    // The window is black and has the image view that shows the frames in
    // it. The video is shown rotated by 90 degrees (so it is the right way up
    // when the device is in landscape orientation), like the real movie player
    // does, which is why the width and height are swapped here.
    let screen: id = msg_class![env; UIScreen mainScreen];
    let bounds: CGRect = msg![env; screen bounds];
    let window: id = msg_class![env; UIWindow alloc];
    let window_frame = CGRect {
        origin: CGPoint {
            x: bounds.origin.x,
            y: bounds.origin.y,
        },
        size: CGSize {
            width: bounds.size.width,
            height: bounds.size.height,
        },
    };
    let window: id = msg![env; window initWithFrame:window_frame];
    let black: id = msg_class![env; UIColor blackColor];
    () = msg![env; window setBackgroundColor:black];
    // Touches are for the app, not for the movie.
    () = msg![env; window setUserInteractionEnabled:false];

    let image_frame = fit_rect(&bounds, track.height as f32, track.width as f32);
    let image_view: id = msg_class![env; UIImageView alloc];
    let image_view: id = msg![env; image_view initWithFrame:image_frame];
    () = msg![env; window addSubview:image_view];
    // The window now owns the image view.
    release(env, image_view);

    () = msg![env; window setHidden:false];

    log!(
        "Showing the video of the movie ({}x{}, {:.2} seconds)",
        track.width,
        track.height,
        duration,
    );
    let video = Box::new(VideoPlayback {
        file,
        track,
        decoder,
        next_sample: 0,
        started: Instant::now(),
        window,
        image_view,
        shown_any: false,
        failed: false,
    });
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(player)
        .video = Some(video);
    Some(duration.min(6.0 * 60.0 * 60.0))
}

/// Put the decoder's latest frame in the image view.
fn show_frame(env: &mut Environment, video: &VideoPlayback) {
    let Some(frame) = video.decoder.frame() else {
        return;
    };
    let (width, height) = (frame.width, frame.height);
    // The picture is rotated one way or the other depending on how the whole
    // screen is going to be rotated, so that the result is always the right
    // way up for landscape orientations.
    let clockwise = !matches!(
        env.window().current_rotation(),
        DeviceOrientation::LandscapeRight
    );
    let mut rgba = vec![0u8; width * height * 4];
    frame.write_rgba_rotated(&mut rgba, clockwise);
    let image = Image::from_pixel_vec(rgba, (height as u32, width as u32));
    let cg_image = cg_image::from_image(env, image);
    let ui_image: id = msg_class![env; UIImage imageWithCGImage:cg_image];
    let image_view = video.image_view;
    () = msg![env; image_view setImage:ui_image];
    // The UIImage keeps the CGImage alive.
    cg_image::CGImageRelease(env, cg_image);
}

/// Decode the frames that are due and show the latest one.
fn step_video(env: &mut Environment, player: id) {
    let Some(mut video) = env
        .objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(player)
        .video
        .take()
    else {
        return;
    };
    if !video.failed {
        let elapsed = video.started.elapsed().as_secs_f64();
        let timescale = f64::from(video.track.timescale.max(1));
        let mut decoded_any = false;
        let mut count = 0;
        while video.next_sample < video.track.samples.len() && count < MAX_FRAMES_PER_STEP {
            let index = video.next_sample;
            if video.track.samples[index].start as f64 / timescale > elapsed {
                break;
            }
            video.next_sample += 1;
            count += 1;
            let Some(data) = video.track.sample_data(&video.file, index) else {
                continue;
            };
            match video.decoder.decode_packet(data) {
                Ok(true) => decoded_any = true,
                Ok(false) => {}
                Err(error) => {
                    log!("Couldn't decode frame {} of the movie: {:?}", index, error);
                    video.failed = true;
                    break;
                }
            }
        }
        if decoded_any {
            show_frame(env, &video);
            video.shown_any = true;
        }
    }
    let give_up = video.failed && !video.shown_any;
    env.objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(player)
        .video = Some(video);
    if give_up {
        // Nothing could be shown, so let the app's own screen be seen.
        stop_video(env, player);
    }
}

/// Stop showing the movie's video and get rid of its window.
fn stop_video(env: &mut Environment, player: id) {
    let Some(video) = env
        .objc
        .borrow_mut::<MPMoviePlayerControllerHostObject>(player)
        .video
        .take()
    else {
        return;
    };
    let window = video.window;
    () = msg![env; window setHidden:true];
    release(env, window);
}

/// Called every time the run loop comes around: shows the video frames that
/// are due.
fn step_players(env: &mut Environment) {
    let Some(player) = State::get(env).active_player else {
        return;
    };
    step_video(env, player);
}
