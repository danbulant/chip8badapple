use ffmpeg_next::format::{input, Pixel};
use ffmpeg_next::media::Type;
use ffmpeg_next::software::scaling::{flag::Flags};
use ffmpeg_next::util::frame::video::Video;
use std::env;
use std::fs::File;
use std::io::{Seek, SeekFrom, Write};

const WIDTH: u8 = 64;
const HEIGHT: u8 = 32;

fn main() -> Result<(), ffmpeg_next::Error> {
    ffmpeg_next::init().unwrap();

    if let Ok(mut ictx) = input(&env::args().nth(1).expect("Cannot open file.")) {
        let input = ictx
            .streams()
            .best(Type::Video)
            .ok_or(ffmpeg_next::Error::StreamNotFound)?;
        let video_stream_index = input.index();

        let mut output_file = File::create("output.ch8").unwrap();

        let context_decoder = ffmpeg_next::codec::context::Context::from_parameters(input.parameters())?;
        let mut decoder = context_decoder.decoder().video()?;

        // clion doesn't like use for this...
        let mut scaler = ffmpeg_next::software::scaling::context::Context::get(
            // input
            decoder.format(),
            decoder.width(),
            decoder.height(),
            // output
            Pixel::GRAY8,
            WIDTH as u32,
            HEIGHT as u32,
            Flags::BICUBIC
        )?;

        let mut frame_index = 0;

        // the below is the main program loop. It's simple, so it's hardcoded.
        // V0=x, V1=y, V10=VA=frame count, V11=VB=delay timer
        // it uses sprite bank to draw sprites on the screen
        // because of frame counting, maximum length is roughly 36.4 minutes (for 30fps)
        let base_rom: [u8; 50] = [
            0xA3,0x00, // 0x200 I = 0x300
            0x6C,0x01, // 0x202 VC = 1

            // main print loop
            0x00,0xE0, // 0x204 clear screen,
            0x61,0x00, // 0x206 V1 = 0
            0x60,0x00, // 0x208 V0 = 0
            0x31,  30, // 0x20A if V1 == 30 skip
            0xD0,0x1F, // 0x20C draw sprite at V0, V1 with height 15
            0x41,  30, // 0x20E if V1 != 30 skip
            0xD0,0x12, // 0x210 draw sprite at V0, V1 with height 2
            0x70,0x08, // 0x212 V0 += 8
            0x62, 120, // 0x214 V2 = 120
            0xF2,0x1E, // 0x216 I += V2

            0x30,WIDTH, // 0x218 if V0 == WIDTH (64) skip
            0x12,0x0A, // 0x21A goto 0x20A
            0x71,  15, // 0x21C V1 += 15
            0x31,  45, // 0x21E if V1 == 45 skip
            0x12,0x08, // 0x220 goto 0x208

            // wait for next frame. Delay timer ticks every 1/60th of a second, video is 30fps, hence 2 ticks
            0x6B,0x02, // 0x222 VB = 2
            0xFB,0x15, // 0x224 delay timer = VB
            0xFB,0x07, // 0x226 VB = delay timer

            0x4B,0x00, // 0x228 if VB != 0 skip
            0x12,0x22, // 0x22A goto 0x222 (repeat wait)

            0x12,0x04, // 0x22C goto 0x204
            // unreachable, for future use
            0x12,0x2E, // 0x22E goto 0x22E (self jump, our emulator exits)
            0x00,0x00 // crash if we get here
        ];

        // dbg!(&base_rom);
        output_file.write_all(&base_rom).unwrap();
        output_file.seek(std::io::SeekFrom::Start(0x100)).unwrap();

        let mut receive_and_process_decoded_frames =
            |decoder: &mut ffmpeg_next::decoder::Video| -> Result<(), ffmpeg_next::Error> {
                let mut decoded = Video::empty();
                while decoder.receive_frame(&mut decoded).is_ok() {
                    let mut bw_frame = Video::empty();
                    scaler.run(&decoded, &mut bw_frame)?;
                    let data = bw_frame.data(0);
                    // dbg!(data);
                    let mut sprite_bank: [[[u8; 15]; 8]; 3] = [
                        [
                            [
                                0; 15 // 15 pixel tall, 8 pixel wide (u8) sprite
                            ]; 8 // there are 8 sprites in 64 pixel wide screen
                        ]; 3 // 3 parts 15 pixels each to fill 32 pixel tall screen
                    ];

                    for horizontal_part in 0..8 {
                        for vertical_part in 0..3 {
                            let rows = if vertical_part == 2 { 2 } else { 15 };
                            for row in 0..rows {
                                // data is 64x32x8bit, we need to split it into 8x15x8bit chunks
                                for col in 0..8 {
                                    let x = horizontal_part * 8 + col;
                                    let y = vertical_part * 15 + row;
                                    let pixel = data[y as usize * WIDTH as usize + x as usize];
                                    // dbg!(pixel);
                                    if pixel > 30 {
                                        sprite_bank[vertical_part][horizontal_part][row] |= 1 << (7 - col);
                                    }
                                }
                            }
                        }
                    }

                    // let mut file = File::create(format!("frames/f{}.pbm", frame_index)).unwrap();
                    // file.write_all(format!("P5\n{} {}\n255\n", WIDTH, HEIGHT).as_bytes()).unwrap();
                    // file.write_all(data).unwrap();

                    for part in 0..3 {
                        for line in 0..15 {
                            for sprite in 0..8 {
                                print!("{:08b} ", sprite_bank[part][sprite][line]);
                            }
                            println!();
                        }
                        println!();
                    }

                    println!();
                    println!("frame {}", frame_index);
                    println!();

                    for part in 0..3 {
                        for sprite in 0..8 {
                            output_file.write_all(&sprite_bank[part][sprite]).unwrap();
                        }
                    }

                    frame_index += 1;
                }
                Ok(())
            };

        for (stream, packet) in ictx.packets() {
            if stream.index() == video_stream_index {

                decoder.send_packet(&packet)?;
                receive_and_process_decoded_frames(&mut decoder)?;
            }
        }
        decoder.send_eof()?;
        receive_and_process_decoded_frames(&mut decoder)?;
    }

    Ok(())
}