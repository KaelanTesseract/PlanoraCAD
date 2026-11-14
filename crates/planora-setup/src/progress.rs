//! Fortschrittsdatei des Motors: zwei Zeilen `percent=<0-100>` und
//! `step=<prepare|install|postinstall|done>`.

/// Phase des Motors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Prepare,
    Install,
    PostInstall,
    Done,
}

/// Ein gelesener Stand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub percent: u8,
    pub step: Step,
}

/// Liest den Dateitext; `None` bei leerem, halb geschriebenem oder unlesbarem Inhalt.
pub fn parse(text: &str) -> Option<Progress> {
    let mut percent = None;
    let mut step = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "percent" => percent = value.parse::<u8>().ok().filter(|p| *p <= 100),
            "step" => {
                step = match value {
                    "prepare" => Some(Step::Prepare),
                    "install" => Some(Step::Install),
                    "postinstall" => Some(Step::PostInstall),
                    "done" => Some(Step::Done),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    Some(Progress {
        percent: percent?,
        step: step?,
    })
}

/// Hält den besten Stand: Prozent und Phase gehen nie zurück.
#[derive(Clone, Copy, Debug)]
pub struct Tracker {
    best: Progress,
}

impl Tracker {
    pub fn new() -> Tracker {
        Tracker {
            best: Progress {
                percent: 0,
                step: Step::Prepare,
            },
        }
    }

    pub fn update(&mut self, p: Progress) {
        self.best.percent = self.best.percent.max(p.percent);
        self.best.step = self.best.step.max(p.step);
    }

    pub fn current(&self) -> Progress {
        self.best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_never_goes_backwards_and_ignores_garbage() {
        let mut t = Tracker::new();
        for text in ["percent=40\nstep=install", "", "percent=3", "percent=x"] {
            if let Some(p) = parse(text) {
                t.update(p);
            }
        }
        assert_eq!(
            t.current(),
            Progress {
                percent: 40,
                step: Step::Install
            }
        );
        // Ein niedrigerer, aber vollständiger Stand ändert nichts.
        t.update(parse("percent=10\r\nstep=prepare\r\n").unwrap());
        assert_eq!(t.current().percent, 40);
        assert_eq!(t.current().step, Step::Install);
        t.update(parse("percent=100\nstep=done").unwrap());
        assert_eq!(
            t.current(),
            Progress {
                percent: 100,
                step: Step::Done
            }
        );
    }

    #[test]
    fn parse_rejects_out_of_range_and_unknown_step() {
        assert_eq!(parse("percent=101\nstep=install"), None);
        assert_eq!(parse("percent=5\nstep=bogus"), None);
        assert_eq!(parse("step=install"), None);
    }
}
