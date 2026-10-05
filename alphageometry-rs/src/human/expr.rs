use super::model::Expr;
use super::trace::{EngineTrace, Table};
use crate::elimination::ANGLE_UNIT;
use crate::lincomb::LinComb;
use crate::predicate::PointId;
use crate::rational::Rat;

pub fn interior(t: &EngineTrace, a: PointId, b: PointId, c: PointId) -> Option<LinComb> {
    let raw = &t.dir(b, c)? - &t.dir(b, a)?;
    let (pa, pb, pc) = (t.coord(a), t.coord(b), t.coord(c));
    let (u, w) = ((pa.0 - pb.0, pa.1 - pb.1), (pc.0 - pb.0, pc.1 - pb.1));
    let nu = (u.0 * u.0 + u.1 * u.1).sqrt();
    let nw = (w.0 * w.0 + w.1 * w.1).sqrt();
    if nu < 1e-12 || nw < 1e-12 {
        return None;
    }
    let theta = ((u.0 * w.0 + u.1 * w.1) / (nu * nw)).clamp(-1.0, 1.0).acos() / std::f64::consts::PI;
    let phi = t.value(Table::Angle, &raw);
    for s in [1i64, -1] {
        let m = theta - s as f64 * phi;
        if (m - m.round()).abs() < 1e-7 {
            let mut out = raw.clone();
            out.mul_assign_scalar(&Rat::from_int(s));
            out.add_term(ANGLE_UNIT, Rat::from_int(m.round() as i64));
            return Some(out);
        }
    }
    None
}

pub fn sine_var(t: &EngineTrace, angle: &Expr, cos: bool) -> Option<LinComb> {
    let (a, b, c, flip, shift) = match angle {
        Expr::Angle { a, b, c, .. } => (*a, *b, *c, false, if cos { Rat::new(1, 2) } else { Rat::zero() }),
        Expr::Lin { terms } if !cos && terms.len() == 2 => match (&terms[0], &terms[1]) {
            ((k, Expr::Angle { a, b, c, .. }), (one, Expr::Const { degrees })) if one.is_one() && k.abs().is_one() => {
                (*a, *b, *c, k.is_negative(), (degrees / &Rat::from_int(180)).mod_one())
            }
            _ => return None,
        },
        _ => return None,
    };
    let probe = super::trace::SineVar { var: 0, v: b, p: a, q: c, flip, shift };
    let key = EngineTrace::sine_key(&probe);
    let v = t.sines.iter().filter(|s| EngineTrace::sine_key(s) == key).map(|s| s.var).min()?;
    Some(LinComb::singleton(v, Rat::one()))
}

pub fn eval(t: &EngineTrace, e: &Expr) -> Option<(Table, LinComb)> {
    match e {
        Expr::Angle { a, b, c, directed } => {
            if *directed {
                Some((Table::Angle, &t.dir(*b, *c)? - &t.dir(*b, *a)?))
            } else {
                Some((Table::Angle, interior(t, *a, *b, *c)?))
            }
        }
        Expr::LineAngle { l1, l2, .. } => Some((Table::Angle, &t.dir(l2.0, l2.1)? - &t.dir(l1.0, l1.1)?)),
        Expr::Const { degrees } => Some((Table::Angle, LinComb::singleton(ANGLE_UNIT, degrees / &Rat::from_int(180)))),
        Expr::Lin { terms } => {
            let mut out = LinComb::zero();
            let mut table = Table::Angle;
            for (k, x) in terms {
                let (tb, c) = eval(t, x)?;
                table = tb;
                out.iadd_mul(&c, k);
            }
            Some((table, out))
        }
        Expr::Seg { a, b } => Some((Table::Ratio, t.dm(*a, *b)?)),
        Expr::Sq { a, b } => Some((Table::Sq, t.single(Table::Sq, *a, *b)?)),
        Expr::Prod { factors } => {
            let mut out = LinComb::zero();
            for (x, k) in factors {
                let (_, c) = eval(t, x)?;
                out.iadd_mul(&c, &Rat::from_int(*k as i64));
            }
            Some((Table::Ratio, out))
        }
        Expr::Sin { angle } => Some((Table::Ratio, sine_var(t, angle, false)?)),
        Expr::Cos { angle } => Some((Table::Ratio, sine_var(t, angle, true)?)),
        Expr::Num { value } => Some((Table::Ratio, t.prime_const(value)?)),
    }
}

pub fn is_directed(e: &Expr) -> bool {
    match e {
        Expr::Angle { directed, .. } | Expr::LineAngle { directed, .. } => *directed,
        Expr::Lin { terms } => terms.iter().all(|(_, x)| is_directed(x)),
        _ => true,
    }
}

pub fn has_angle(e: &Expr) -> bool {
    match e {
        Expr::Angle { .. } | Expr::LineAngle { .. } => true,
        Expr::Lin { terms } => terms.iter().any(|(_, x)| has_angle(x)),
        _ => false,
    }
}
