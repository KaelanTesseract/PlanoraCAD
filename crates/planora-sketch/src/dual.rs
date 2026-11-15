//! Zahlen mit Gradient (Vorwärts-Ableitung): Jede Rechnung liefert den Wert und die exakten
//! partiellen Ableitungen nach den lokalen Variablen.

use std::ops::{Add, Deref, DerefMut, Div, Mul, Neg, Sub};

/// Untere Schranke für die Wurzel im Nenner der Ableitung von `sqrt` (Nullabstände).
const MIN_ROOT: f64 = 1e-12;

/// So viele Ableitungen hält ein [`Grad`] ohne eigenen Speicher auf dem Heap.
const INLINE: usize = 16;

/// Gradient: die partiellen Ableitungen nach den lokalen Variablen. Bis [`INLINE`] Einträge
/// liegen sie im Wert selbst, darüber in einem `Vec` (Skizzen 11a: Jede Rechnung mit `Dual`
/// legte sonst einen neuen `Vec` an, das kostete beim Ziehen großer Gruppen die meiste Zeit).
/// Die Zahlen sind dieselben wie mit `Vec<f64>`.
#[derive(Clone)]
pub enum Grad {
    /// Leer (Konstante), ohne die Einträge anzulegen.
    Empty,
    Inline(usize, [f64; INLINE]),
    Heap(Vec<f64>),
}

impl Grad {
    /// Leer (Konstante).
    pub fn new() -> Grad {
        Grad::Empty
    }

    /// `n` Nullen.
    pub fn zeros(n: usize) -> Grad {
        if n <= INLINE {
            Grad::Inline(n, [0.0; INLINE])
        } else {
            Grad::Heap(vec![0.0; n])
        }
    }
}

impl Default for Grad {
    fn default() -> Grad {
        Grad::new()
    }
}

impl Deref for Grad {
    type Target = [f64];
    fn deref(&self) -> &[f64] {
        match self {
            Grad::Empty => &[],
            Grad::Inline(n, a) => &a[..*n],
            Grad::Heap(v) => v,
        }
    }
}

impl DerefMut for Grad {
    fn deref_mut(&mut self) -> &mut [f64] {
        match self {
            Grad::Empty => &mut [],
            Grad::Inline(n, a) => &mut a[..*n],
            Grad::Heap(v) => v,
        }
    }
}

impl FromIterator<f64> for Grad {
    fn from_iter<I: IntoIterator<Item = f64>>(iter: I) -> Grad {
        let iter = iter.into_iter();
        match iter.size_hint() {
            (_, Some(0)) => Grad::Empty,
            (_, Some(hi)) if hi <= INLINE => {
                let mut a = [0.0; INLINE];
                let mut n = 0;
                for (slot, v) in a.iter_mut().zip(iter) {
                    *slot = v;
                    n += 1;
                }
                Grad::Inline(n, a)
            }
            _ => Grad::Heap(iter.collect()),
        }
    }
}

impl From<Vec<f64>> for Grad {
    fn from(v: Vec<f64>) -> Grad {
        Grad::Heap(v)
    }
}

impl PartialEq for Grad {
    fn eq(&self, other: &Grad) -> bool {
        **self == **other
    }
}

impl std::fmt::Debug for Grad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(f)
    }
}

/// Wert mit Gradient. Ein leerer Gradient bedeutet „Konstante“ (alle Ableitungen null).
#[derive(Clone, Debug, PartialEq)]
pub struct Dual {
    pub v: f64,
    pub d: Grad,
}

impl Dual {
    pub fn constant(v: f64) -> Dual {
        Dual { v, d: Grad::new() }
    }

    /// Variable Nummer `index` von insgesamt `n` lokalen Variablen.
    pub fn variable(v: f64, index: usize, n: usize) -> Dual {
        let mut d = Grad::zeros(n);
        d[index] = 1.0;
        Dual { v, d }
    }

    pub fn grad(&self, i: usize) -> f64 {
        self.d.get(i).copied().unwrap_or(0.0)
    }

    pub fn sqrt(&self) -> Dual {
        let s = self.v.max(0.0).sqrt();
        let den = 2.0 * s.max(MIN_ROOT);
        Dual {
            v: s,
            d: self.d.iter().map(|x| x / den).collect(),
        }
    }

    /// Winkel `atan2(self, x)`; an der Stelle (0, 0) ohne Gradient.
    pub fn atan2(&self, x: &Dual) -> Dual {
        let den = self.v * self.v + x.v * x.v;
        let (cy, cx) = if den > 0.0 {
            (x.v / den, -self.v / den)
        } else {
            (0.0, 0.0)
        };
        lin(self, cy, x, cx, self.v.atan2(x.v))
    }

    /// Sinus; Ableitung `cos · Gradient`.
    pub fn sin(&self) -> Dual {
        let c = self.v.cos();
        Dual {
            v: self.v.sin(),
            d: self.d.iter().map(|x| x * c).collect(),
        }
    }

    /// Kosinus; Ableitung `−sin · Gradient`.
    pub fn cos(&self) -> Dual {
        let s = self.v.sin();
        Dual {
            v: self.v.cos(),
            d: self.d.iter().map(|x| -x * s).collect(),
        }
    }
}

/// `ca·a + cb·b` im Gradienten, Wert `v`.
fn lin(a: &Dual, ca: f64, b: &Dual, cb: f64, v: f64) -> Dual {
    let n = a.d.len().max(b.d.len());
    if n == 0 {
        return Dual { v, d: Grad::Empty };
    }
    if n <= INLINE {
        // Dieselbe Rechnung wie unten, ohne Umweg über einen Iterator.
        let mut out = [0.0; INLINE];
        for (i, o) in out[..n].iter_mut().enumerate() {
            *o = ca * a.grad(i) + cb * b.grad(i);
        }
        return Dual {
            v,
            d: Grad::Inline(n, out),
        };
    }
    Dual {
        v,
        d: (0..n).map(|i| ca * a.grad(i) + cb * b.grad(i)).collect(),
    }
}

fn add_dual(a: &Dual, b: &Dual) -> Dual {
    lin(a, 1.0, b, 1.0, a.v + b.v)
}
fn sub_dual(a: &Dual, b: &Dual) -> Dual {
    lin(a, 1.0, b, -1.0, a.v - b.v)
}
fn mul_dual(a: &Dual, b: &Dual) -> Dual {
    lin(a, b.v, b, a.v, a.v * b.v)
}
fn div_dual(a: &Dual, b: &Dual) -> Dual {
    let q = a.v / b.v;
    lin(a, 1.0 / b.v, b, -q / b.v, q)
}

macro_rules! binop {
    ($tr:ident, $m:ident, $f:ident) => {
        impl $tr<&Dual> for &Dual {
            type Output = Dual;
            fn $m(self, rhs: &Dual) -> Dual {
                $f(self, rhs)
            }
        }
        impl $tr<Dual> for Dual {
            type Output = Dual;
            fn $m(self, rhs: Dual) -> Dual {
                $f(&self, &rhs)
            }
        }
        impl $tr<&Dual> for Dual {
            type Output = Dual;
            fn $m(self, rhs: &Dual) -> Dual {
                $f(&self, rhs)
            }
        }
        impl $tr<Dual> for &Dual {
            type Output = Dual;
            fn $m(self, rhs: Dual) -> Dual {
                $f(self, &rhs)
            }
        }
        impl $tr<f64> for &Dual {
            type Output = Dual;
            fn $m(self, rhs: f64) -> Dual {
                $f(self, &Dual::constant(rhs))
            }
        }
        impl $tr<f64> for Dual {
            type Output = Dual;
            fn $m(self, rhs: f64) -> Dual {
                $f(&self, &Dual::constant(rhs))
            }
        }
        impl $tr<&Dual> for f64 {
            type Output = Dual;
            fn $m(self, rhs: &Dual) -> Dual {
                $f(&Dual::constant(self), rhs)
            }
        }
        impl $tr<Dual> for f64 {
            type Output = Dual;
            fn $m(self, rhs: Dual) -> Dual {
                $f(&Dual::constant(self), &rhs)
            }
        }
    };
}
binop!(Add, add, add_dual);
binop!(Sub, sub, sub_dual);
binop!(Mul, mul, mul_dual);
binop!(Div, div, div_dual);

impl Neg for Dual {
    type Output = Dual;
    fn neg(self) -> Dual {
        -&self
    }
}
impl Neg for &Dual {
    type Output = Dual;
    fn neg(self) -> Dual {
        Dual {
            v: -self.v,
            d: self.d.iter().map(|x| -x).collect(),
        }
    }
}
