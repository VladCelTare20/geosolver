use super::*;
use crate::human::trace::{EngineTrace, SineVar};
use crate::lincomb::VarId;

impl Ddar {
    pub fn trace(&self) -> EngineTrace {
        let n = self.n;
        let one = |c: &LinComb| (c.terms.len() == 1 && c.terms[0].1.is_one()).then(|| c.terms[0].0);
        let dir: Vec<Option<VarId>> = self.pair_dir.iter().map(|x| x.as_ref().and_then(|a| one(&a.0))).collect();
        let dm: Vec<Option<VarId>> = self.pair_dist_mul.iter().map(|x| x.as_ref().and_then(|a| one(&a.0))).collect();
        let da: Vec<Option<VarId>> = self.pair_dist_add.iter().map(|x| x.as_ref().and_then(|a| one(&a.0))).collect();
        let ds: Vec<Option<VarId>> = self.pair_dist_sq.iter().map(|x| x.as_ref().and_then(|a| one(&a.0))).collect();
        let mut var_pair: FxHashMap<VarId, (PointId, PointId)> = FxHashMap::default();
        for a in 0..n {
            for b in 0..n {
                if let Some(v) = dir[a * n + b] {
                    var_pair.entry(v).or_insert((a as PointId, b as PointId));
                }
            }
        }
        let corner_of = |dp: VarId, dq: VarId| -> Option<(PointId, PointId, PointId)> {
            let (a, b) = *var_pair.get(&dp)?;
            let (c, d) = *var_pair.get(&dq)?;
            if a == c {
                Some((a, b, d))
            } else if a == d {
                Some((a, b, c))
            } else if b == c {
                Some((b, a, d))
            } else if b == d {
                Some((b, a, c))
            } else {
                None
            }
        };
        let mut sines: Vec<SineVar> = Vec::new();
        for (key, s) in &self.trig.svar {
            if let (Some(var), Some((v, p, q))) = (one(&s.0), corner_of(key.0, key.1)) {
                sines.push(SineVar { var, v, p, q, flip: false, shift: Rat::zero() });
            }
        }
        for (class, s) in &self.trig.vsvar {
            if let (Some(var), Some(r)) = (one(&s.0), self.trig.vref.get(class)) {
                sines.push(SineVar { var, v: r.c.v, p: r.c.p, q: r.c.q, flip: r.flip, shift: r.shift.clone() });
            }
        }
        sines.sort_by_key(|s| s.var);
        sines.dedup_by_key(|s| s.var);
        let mut primes: Vec<(u64, VarId)> = Vec::new();
        for (v, &is_lhs) in self.dmul.core.is_lhs.iter().enumerate() {
            if !is_lhs {
                let val = self.dmul.core.values[v];
                if val >= 2.0 && val.fract() == 0.0 {
                    primes.push((val as u64, v as VarId));
                }
            }
        }
        primes.sort();
        EngineTrace {
            n,
            names: self.names.clone(),
            coords: self.coords.iter().map(|c| (c.x, c.y)).collect(),
            subst: self.subst.clone(),
            facts: self.log.facts.clone(),
            rows: [
                self.angle.core.fact_rows.clone(),
                self.dmul.core.fact_rows.clone(),
                self.dadd.core.fact_rows.clone(),
                self.dsq.core.fact_rows.clone(),
            ],
            values: [
                self.angle.core.values.clone(),
                self.dmul.core.values.clone(),
                self.dadd.core.values.clone(),
                self.dsq.core.values.clone(),
            ],
            lhs: [
                self.angle.core.is_lhs.clone(),
                self.dmul.core.is_lhs.clone(),
                self.dadd.core.is_lhs.clone(),
                self.dsq.core.is_lhs.clone(),
            ],
            pair: [dir, dm, da, ds],
            primes,
            sines,
        }
    }
}
