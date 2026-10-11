//! Kleines dichtes Gleichungssystem `A·x = b` (Gauß-Elimination mit Spaltenpivot) für Spline-
//! Interpolation und Ausgleichsrechnung.

use super::NurbsError;
use super::fit::MAX_FIT_CONTROL_POINTS;

/// Ist der Betrag des besten Pivots kleiner, gilt das System als singulär.
const SINGULAR_PIVOT: f64 = 1e-300;
/// Ist der Betrag des besten Pivots kleiner als dieser Bruchteil des größten Matrixeintrags, gilt
/// das System als numerisch singulär (die Lösung wäre von Rundung beherrscht). Die Systeme hier
/// (Kollokations- und Normalmatrizen von B-Splines mit gemittelten Knoten) haben Pivots weit
/// darüber.
const SINGULAR_PIVOT_RATIO: f64 = 1e-12;

/// Löst `A·x = b` an Ort und Stelle: `a` ist zeilenweise `n × n`, `b` hat `n` Einträge und enthält
/// danach die Lösung; `a` wird dabei überschrieben. Für mehrere rechte Seiten wird die Funktion je
/// Seite mit einer Kopie der Matrix aufgerufen.
pub(crate) fn solve_dense(a: &mut [f64], b: &mut [f64], n: usize) -> Result<(), NurbsError> {
    if n == 0 || n > MAX_FIT_CONTROL_POINTS {
        return Err(NurbsError::TooLarge(format!(
            "Gleichungssystem mit {n} Unbekannten (erlaubt: 1 bis {MAX_FIT_CONTROL_POINTS})"
        )));
    }
    if a.len() != n * n || b.len() != n {
        return Err(NurbsError::Input(
            "Matrix und rechte Seite passen nicht zur Größe des Gleichungssystems".into(),
        ));
    }
    if a.iter().chain(b.iter()).any(|v| !v.is_finite()) {
        return Err(NurbsError::Fit(
            "Gleichungssystem enthält nicht endliche Zahlen".into(),
        ));
    }
    let scale = a.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let min_pivot = SINGULAR_PIVOT.max(SINGULAR_PIVOT_RATIO * scale);
    for col in 0..n {
        let (piv, best) = (col..n)
            .map(|r| (r, a[r * n + col].abs()))
            .fold((col, -1.0), |acc, x| if x.1 > acc.1 { x } else { acc });
        if best < min_pivot {
            return Err(NurbsError::Fit("Gleichungssystem ist singulär".into()));
        }
        if piv != col {
            for k in 0..n {
                a.swap(piv * n + k, col * n + k);
            }
            b.swap(piv, col);
        }
        let d = a[col * n + col];
        for r in col + 1..n {
            let f = a[r * n + col] / d;
            if f == 0.0 {
                continue;
            }
            a[r * n + col] = 0.0;
            for k in col + 1..n {
                a[r * n + k] -= f * a[col * n + k];
            }
            b[r] -= f * b[col];
        }
    }
    for r in (0..n).rev() {
        let mut s = b[r];
        for k in r + 1..n {
            s -= a[r * n + k] * b[k];
        }
        b[r] = s / a[r * n + r];
    }
    if b.iter().any(|v| !v.is_finite()) {
        return Err(NurbsError::Fit(
            "Gleichungssystem ist numerisch singulär (Lösung nicht endlich)".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solves_with_pivoting() {
        // erste Spalte beginnt mit 0: ohne Tausch ginge es nicht
        let mut a = vec![0.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 3.0];
        let mut b = vec![7.0, 6.0, 13.0];
        solve_dense(&mut a, &mut b, 3).unwrap();
        for (x, e) in b.iter().zip([1.0, 2.0, 3.0]) {
            assert!((x - e).abs() < 1e-12, "{b:?}");
        }
    }

    #[test]
    fn rejects_singular_bad_sizes_and_nan() {
        let mut a = vec![1.0, 2.0, 2.0, 4.0];
        assert!(solve_dense(&mut a, &mut [1.0, 2.0], 2).is_err());
        // fast singulär (relativer Pivot 1e-14): Fehler statt Rauschen
        let mut a = vec![1.0, 1.0, 1.0, 1.0 + 1e-14];
        assert!(solve_dense(&mut a, &mut [1.0, 2.0], 2).is_err());
        assert!(solve_dense(&mut [1.0], &mut [1.0, 2.0], 2).is_err());
        assert!(solve_dense(&mut [], &mut [], 0).is_err());
        assert!(solve_dense(&mut [f64::NAN], &mut [1.0], 1).is_err());
        let n = MAX_FIT_CONTROL_POINTS + 1;
        assert!(solve_dense(&mut vec![0.0; n * n], &mut vec![0.0; n], n).is_err());
    }
}
