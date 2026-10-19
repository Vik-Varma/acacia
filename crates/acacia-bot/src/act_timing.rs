//! Where an act's time goes: wall time per phase, kept per bot ([`crate::Bot::act_timing`]). Measurement only;
//! nothing reads it back. Printed by the live build examples.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// The processes deciding (builder survey and plan included), CPU.
    Plan,
    /// One whole act, as the process loop awaits it.
    Act,
    /// Equip with the right item already in hand.
    EquipSame,
    /// Equip that switched the held item (hotbar or a move from the inventory), human pause included.
    EquipSwitch,
    /// Turning to the click point.
    Aim,
    /// Waiting for the governor to let the action out.
    Admit,
    /// Send to the confirmation's verdict (echo wait, retries included).
    Echo,
    /// Mining ticks of a break.
    Mine,
    /// A restock: open, take, close.
    Restock,
    /// The tick the process loop lets pass after an act.
    PostTick,
}

const PHASES: [Phase; 10] = [Phase::Plan, Phase::Act, Phase::EquipSame, Phase::EquipSwitch, Phase::Aim, Phase::Admit, Phase::Echo, Phase::Mine, Phase::Restock, Phase::PostTick];

impl crate::Bot {
    pub fn act_timing(&self) -> &ActTiming {
        &self.act_timing
    }
}

#[derive(Debug, Clone, Default)]
pub struct ActTiming {
    samples: [Vec<Duration>; PHASES.len()],
    retries: u32,
}

impl ActTiming {
    pub(crate) fn add(&mut self, phase: Phase, d: Duration) {
        self.samples[phase as usize].push(d);
    }

    pub(crate) fn retry(&mut self) {
        self.retries += 1;
    }

    pub fn samples(&self, phase: Phase) -> &[Duration] {
        &self.samples[phase as usize]
    }

    /// Hotbar or inventory switches of the held item.
    pub fn switches(&self) -> usize {
        self.samples(Phase::EquipSwitch).len()
    }

    pub fn retries(&self) -> u32 {
        self.retries
    }

    /// One line per phase: count, total, mean, p50, p95 (ms).
    pub fn table(&self) -> String {
        let mut out = format!("{:<12} {:>6} {:>9} {:>8} {:>8} {:>8}\n", "phase", "n", "total s", "mean ms", "p50 ms", "p95 ms");
        for p in PHASES {
            let mut v = self.samples(p).to_vec();
            if v.is_empty() {
                continue;
            }
            v.sort();
            let total: Duration = v.iter().sum();
            let ms = |d: Duration| d.as_secs_f64() * 1000.0;
            let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
            out += &format!("{:<12} {:>6} {:>9.1} {:>8.1} {:>8.1} {:>8.1}\n", format!("{p:?}"), v.len(), total.as_secs_f64(), ms(total) / v.len() as f64, ms(at(0.5)), ms(at(0.95)));
        }
        out + &format!("switches {}, retries {}", self.switches(), self.retries)
    }
}
