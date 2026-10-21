//! Prints the `PlayerAuthInput`s a movement trace sent for input ticks `from..=to`: rotation, camera,
//! position, delta and move vectors, to line them up with the corrections `replay` reports.
//! `cargo run -p acacia-bot --example trace_inputs -- <trace> <from> <to>`
use acacia_bot::client::proto::Packet;
use acacia_bot::client::proto::packets::PlayerAuthInput;
use acacia_bot::trace::{self, Event};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let usage = "usage: trace_inputs <trace> <from> <to>";
    let path = args.next().ok_or(usage)?;
    let from: u64 = args.next().ok_or(usage)?.parse()?;
    let to: u64 = args.next().ok_or(usage)?.parse()?;
    for event in trace::read(path.as_ref())? {
        let Event::Input(body) = event else { continue };
        let Ok(p) = PlayerAuthInput::decode(&mut &body[..]) else { continue };
        if (from..=to).contains(&p.tick) {
            println!(
                "{} yaw {:.6} head {:.6} irot {:?} cam {:?} pos {:?} delta {:?} mv {:?} raw {:?}",
                p.tick, p.yaw, p.head_yaw, p.interact_rotation, p.camera_orientation, p.position, p.delta, p.move_vector, p.raw_move_vector
            );
        }
    }
    Ok(())
}
