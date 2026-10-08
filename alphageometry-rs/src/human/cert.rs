use crate::lincomb::{LinComb, VarId};
use crate::rational::Rat;
use rustc_hash::FxHashMap;

#[derive(Clone)]
pub struct Basis {
    rows: FxHashMap<VarId, (LinComb, LinComb)>,
    pivotable: Vec<bool>,
    track: bool,
}

impl Basis {
    pub fn new(pivotable: &[bool], track: bool) -> Basis {
        Basis {
            rows: FxHashMap::default(),
            pivotable: pivotable.to_vec(),
            track,
        }
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn can_pivot(&self, v: VarId) -> bool {
        self.pivotable.get(v as usize).copied().unwrap_or(true)
    }

    fn top_below(&self, v: &LinComb, bound: Option<VarId>) -> Option<(VarId, Rat)> {
        v.terms
            .iter()
            .rev()
            .find(|(var, c)| bound.is_none_or(|b| *var < b) && self.can_pivot(*var) && !c.is_zero())
            .map(|(var, c)| (*var, c.clone()))
    }

    pub fn reduce(&self, v: &mut LinComb, combo: &mut LinComb) {
        let mut bound: Option<VarId> = None;
        while let Some((var, c)) = self.top_below(v, bound) {
            if let Some((row, rc)) = self.rows.get(&var) {
                let neg = -&c;
                v.iadd_mul(row, &neg);
                if self.track {
                    combo.iadd_mul(rc, &neg);
                }
            }
            bound = Some(var);
        }
    }

    pub fn residual(&self, t: &LinComb) -> LinComb {
        let mut v = t.clone();
        let mut combo = LinComb::zero();
        self.reduce(&mut v, &mut combo);
        v
    }

    pub fn insert(&mut self, idx: u32, row: &LinComb) -> bool {
        let mut v = row.clone();
        let mut combo = if self.track {
            LinComb::singleton(idx, Rat::one())
        } else {
            LinComb::zero()
        };
        self.reduce(&mut v, &mut combo);
        let Some((p, c)) = self.top_below(&v, None) else {
            return false;
        };
        let s = c.recip();
        v.mul_assign_scalar(&s);
        combo.mul_assign_scalar(&s);
        self.rows.insert(p, (v, combo));
        true
    }

    pub fn contains(&self, t: &LinComb) -> bool {
        self.residual(t).is_zero()
    }

    pub fn solve(&self, t: &LinComb) -> Option<Vec<(u32, Rat)>> {
        let mut v = t.clone();
        let mut combo = LinComb::zero();
        self.reduce(&mut v, &mut combo);
        if !v.is_zero() {
            return None;
        }
        Some(
            combo
                .terms
                .iter()
                .filter(|(_, c)| !c.is_zero())
                .map(|(i, c)| (*i, -c))
                .collect(),
        )
    }
}

pub fn solve_rows(pivotable: &[bool], rows: &[&LinComb], t: &LinComb) -> Option<Vec<(usize, Rat)>> {
    let mut b = Basis::new(pivotable, true);
    for (i, r) in rows.iter().enumerate() {
        b.insert(i as u32, r);
    }
    b.solve(t)
        .map(|v| v.into_iter().map(|(i, c)| (i as usize, c)).collect())
}

pub fn in_span(pivotable: &[bool], rows: &[&LinComb], t: &LinComb) -> bool {
    let mut b = Basis::new(pivotable, false);
    for (i, r) in rows.iter().enumerate() {
        b.insert(i as u32, r);
    }
    b.contains(t)
}

pub fn certify_greedy(
    pivotable: &[bool],
    rows: &[(u32, &LinComb)],
    target: &LinComb,
    removable: &dyn Fn(usize) -> bool,
) -> Option<Vec<(usize, Rat)>> {
    if target.is_zero() {
        return Some(Vec::new());
    }
    let mut b = Basis::new(pivotable, true);
    let mut found = false;
    let mut k = 0;
    while k < rows.len() {
        let level = rows[k].0;
        while k < rows.len() && rows[k].0 == level {
            b.insert(k as u32, rows[k].1);
            k += 1;
        }
        if b.contains(target) {
            found = true;
            break;
        }
    }
    if !found {
        return None;
    }
    let first = b.solve(target)?;
    let mut support: Vec<usize> = first.iter().map(|(i, _)| *i as usize).collect();
    support.sort_unstable();
    let mut order = support.clone();
    order.sort_by_key(|&i| (std::cmp::Reverse(rows[i].0), std::cmp::Reverse(i)));
    for i in order {
        if !removable(i) {
            continue;
        }
        let trial: Vec<usize> = support.iter().copied().filter(|&j| j != i).collect();
        let refs: Vec<&LinComb> = trial.iter().map(|&j| rows[j].1).collect();
        if in_span(pivotable, &refs, target) {
            support = trial;
        }
    }
    let refs: Vec<&LinComb> = support.iter().map(|&j| rows[j].1).collect();
    let lam = solve_rows(pivotable, &refs, target)?;
    Some(lam.into_iter().map(|(k, l)| (support[k], l)).collect())
}

#[derive(Clone)]
pub struct Echelon {
    rows: FxHashMap<VarId, LinComb>,
    pivotable: Vec<bool>,
}

impl Echelon {
    pub fn new(pivotable: &[bool]) -> Echelon {
        Echelon {
            rows: FxHashMap::default(),
            pivotable: pivotable.to_vec(),
        }
    }

    fn key(&self, v: VarId) -> (bool, VarId) {
        (self.pivotable.get(v as usize).copied().unwrap_or(true), v)
    }

    fn reduce(&self, v: &mut LinComb) -> Option<(VarId, Rat)> {
        loop {
            let (var, c) = v
                .terms
                .iter()
                .max_by_key(|(var, _)| self.key(*var))
                .map(|(var, c)| (*var, c.clone()))?;
            if !self.key(var).0 {
                return Some((var, c));
            }
            match self.rows.get(&var) {
                Some(row) => {
                    let neg = -&c;
                    v.iadd_mul(row, &neg);
                }
                None => return Some((var, c)),
            }
        }
    }

    pub fn insert(&mut self, row: &LinComb) -> bool {
        let mut v = row.clone();
        let Some((var, c)) = self.reduce(&mut v) else {
            return false;
        };
        if !self.key(var).0 {
            return false;
        }
        v.mul_assign_scalar(&c.recip());
        self.rows.insert(var, v);
        true
    }

    pub fn contains(&self, t: &LinComb) -> bool {
        let mut v = t.clone();
        self.reduce(&mut v).is_none()
    }
}
