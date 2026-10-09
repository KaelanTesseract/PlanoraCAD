//! Zahlen mit Gradient (Vorwärts-Ableitung): Jede Rechnung liefert den Wert und die exakten
//! partiellen Ableitungen nach den lokalen Variablen.

use std::ops::{Add, Div, Mul, Neg, Sub};

/// Untere Schranke für die Wurzel im Nenner der Ableitung von `sqrt` (Nullabstände).
const MIN_ROOT: f64 = 1e-12;

/// Wert mit Gradient. Ein leerer Gradient bedeutet „Konstante“ (alle Ableitungen null).
#[derive(Clone, Debug, PartialEq)]
pub struct Dual {
    pub v: f64,
    pub d: Vec<f64>,
}

impl Dual {
    pub fn constant(v: f64) -> Dual {
        Dual { v, d: Vec::new() }
    }

    /// Variable Nummer `index` von insgesamt `n` lokalen Variablen.
    pub fn variable(v: f64, index: usize, n: usize) -> Dual {
        let mut d = vec![0.0; n];
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
}

/// `ca·a + cb·b` im Gradienten, Wert `v`.
fn lin(a: &Dual, ca: f64, b: &Dual, cb: f64, v: f64) -> Dual {
    let n = a.d.len().max(b.d.len());
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
