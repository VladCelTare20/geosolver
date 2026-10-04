use super::*;

const MAX_BRIDGE_ATTEMPTS: usize = 2000;
const MAX_TRIG_AUX: usize = 200;
const MAX_BRIDGES: usize = 64;

impl Figure {
    fn mono_val(&self, inst: usize, m: &[LKey]) -> f64 {
        m.iter().map(|&k| self.factor_val(inst, k)).product()
    }

    /// T4: power of a point with respect to a circle known by its centre.
    /// For `P` on a chord `XY`, `PX·PY = σ(OX² − OP²)` with `σ = +1` when `P`
    /// lies between `X` and `Y` in every instance, `−1` when outside in every
    /// instance, and no row when the instances disagree.
    pub(in crate::ratio) fn gather_center_power(&mut self, pts: &[PointId]) {
        for (o, set) in self.circles.clone() {
            let on: Vec<PointId> = set.iter().copied().collect();
            for &p in pts {
                if p == o || set.contains(&p) {
                    continue;
                }
                for (i, &x) in on.iter().enumerate() {
                    for &y in &on[i + 1..] {
                        if !(0..self.insts.len()).all(|k| self.collinear_in(k, p, x, y)) {
                            continue;
                        }
                        let Some(sigma) = self.power_sign(x, p, y) else { continue };
                        if (0..self.insts.len()).any(|k| self.dist(k, o, p) < 1e-9) {
                            continue;
                        }
                        self.push_center_power(o, x, y, p, sigma);
                    }
                }
            }
        }
    }

    fn collinear_in(&self, k: usize, a: PointId, b: PointId, c: PointId) -> bool {
        let (u, v) = (self.insts[k][b as usize] - self.insts[k][a as usize], self.insts[k][c as usize] - self.insts[k][a as usize]);
        (u.x * v.y - u.y * v.x).abs() < 1e-9 * (1.0 + u.norm() * v.norm())
    }

    /// `+1` inside (between the chord ends) in every instance, `−1` outside in
    /// every instance, `None` otherwise.
    pub(in crate::ratio) fn power_sign(&self, x: PointId, p: PointId, y: PointId) -> Option<i64> {
        let side = |k: usize| {
            let (vx, vp, vy) = (self.insts[k][x as usize], self.insts[k][p as usize], self.insts[k][y as usize]);
            let d = (vp - vx).dot(vy - vp);
            let scale = (vy - vx).dot(vy - vx);
            if d > 1e-9 * scale {
                Some(1)
            } else if d < -1e-9 * scale {
                Some(-1)
            } else {
                None
            }
        };
        let first = side(0)?;
        (1..self.insts.len()).all(|k| side(k) == Some(first)).then_some(first)
    }

    fn push_center_power(&mut self, o: PointId, x: PointId, y: PointId, p: PointId, sigma: i64) {
        let (px, py, ox, op) = (latom(p, x), latom(p, y), latom(o, x), latom(o, p));
        let mut e = PEq::default();
        e.add(pmul(px, py), Rat::one());
        e.add(pmul(ox, ox), Rat::from_int(-sigma));
        e.add(pmul(op, op), Rat::from_int(sigma));
        let (pn, on, xn) = (self.nm(p), self.nm(o), self.nm(x));
        let rhs = if sigma > 0 {
            format!("{on}{xn}² − {on}{pn}² ({pn} inside the circle)")
        } else {
            format!("{on}{pn}² − {on}{xn}² ({pn} outside the circle)")
        };
        let intro = self.ppush(
            format!(
                "Power of the point {pn} with respect to the circle ({on}; {on}{xn}): {}·{} = {rhs}.",
                self.seg(p, x),
                self.seg(p, y),
            ),
            None,
            vec![],
        );
        self.pending.push((intro, vec![pred("cong", &[o, x, o, y]), pred("coll", &[p, x, y])], true));
        self.ppush(String::new(), Some(e), vec![intro]);
    }

    /// T3: pool monomials `m1`, `m2` of equal degree whose ratio is the same
    /// small rational `k` in every instance; `ln m1 − ln m2 − ln k = 0` is
    /// proved by the certified log engine on this figure, and the product row
    /// `m1 − k·m2 = 0` carries that derivation. Irrational ratios are never
    /// bridged.
    pub(in crate::ratio) fn gather_log_bridges(&mut self, pool: &BTreeSet<Mono>, goal_monos: &BTreeSet<Mono>) {
        let base: BTreeSet<usize> = (0..self.steps.len()).filter(|&i| self.steps[i].eq.is_some()).collect();
        let monos: Vec<Mono> = pool.iter().cloned().collect();
        let mut pairs: Vec<(usize, usize, Rat)> = Vec::new();
        for i in 0..monos.len() {
            for j in (i + 1)..monos.len() {
                if monos[i].len() != monos[j].len()
                    || !(goal_monos.contains(&monos[i]) || goal_monos.contains(&monos[j]))
                {
                    continue;
                }
                if let Some(k) = self.exact_ratio(&monos[i], &monos[j]) {
                    pairs.push((i, j, k));
                }
            }
        }
        pairs.sort_by_key(|&(i, j, _)| {
            let g = goal_monos.contains(&monos[i]) as u8 + goal_monos.contains(&monos[j]) as u8;
            (std::cmp::Reverse(g), i, j)
        });
        let span = self.echelon(&base);
        let plain: BTreeSet<usize> = base
            .iter()
            .copied()
            .filter(|&i| !self.steps[i].eq.as_ref().unwrap().terms.keys().any(|t| matches!(t, LKey::Sin(..))))
            .collect();
        let plain_span = self.echelon(&plain);
        let mut accepted = 0;
        for (attempt, (i, j, k)) in pairs.into_iter().enumerate() {
            if attempt >= MAX_BRIDGE_ATTEMPTS {
                break;
            }
            let (m1, m2, k) = if k.to_f64() < 1.0 { (&monos[j], &monos[i], k.recip()) } else { (&monos[i], &monos[j], k) };
            let mut goal = LEq::default();
            for &a in m1 {
                goal.add_term(a, Rat::one());
            }
            for &a in m2 {
                goal.add_term(a, -Rat::one());
            }
            for (p, e) in ln_primes(&k).unwrap_or_default() {
                goal.add_prime(p, -e);
            }
            if goal.is_zero() || !in_span(&goal, &span) || in_span(&goal, &plain_span) {
                continue;
            }
            let Some((used, facts)) = self.certified_prove_core(&goal, &base) else { continue };
            if !used.iter().any(|&u| self.steps[u].eq.as_ref().is_some_and(|e| e.terms.keys().any(|t| matches!(t, LKey::Sin(..))))) {
                continue;
            }
            let order: Vec<usize> = used.iter().copied().collect();
            let lines: String = self
                .display_groups(&order)
                .into_iter()
                .map(|(_, t)| format!("\n       · {t}"))
                .collect();
            let kt = if k == Rat::one() { String::new() } else { format!("{}·", rat_text(&k)) };
            let text = format!(
                "From the sine relations below: {} = {kt}{}.{lines}",
                self.mono_text(m1),
                self.mono_text(m2)
            );
            let mut e = PEq::default();
            e.add(m1.clone(), Rat::one());
            e.add(m2.clone(), -k.clone());
            let s = self.ppush(text, Some(e), vec![]);
            self.psteps[s].facts = facts;
            accepted += 1;
            if accepted >= MAX_BRIDGES {
                break;
            }
        }
    }

    /// The rows of `allowed` in echelon form, without provenance: a cheap
    /// necessary test before a certified solve.
    fn echelon(&self, allowed: &BTreeSet<usize>) -> Vec<(LEq, LKey)> {
        let mut rows: Vec<(LEq, LKey)> = Vec::new();
        for &i in allowed {
            let Some(eq0) = &self.steps[i].eq else { continue };
            let mut eq = eq0.clone();
            reduce_plain(&mut eq, &rows);
            if let Some(p) = eq.first_atom() {
                rows.push((eq, p));
            }
        }
        rows
    }

    /// `k` with `m1 = k·m2` in every instance, `k` a small positive rational.
    pub(in crate::ratio) fn exact_ratio(&self, m1: &[LKey], m2: &[LKey]) -> Option<Rat> {
        let r0 = self.mono_val(0, m1) / self.mono_val(0, m2);
        let k = crate::certify::candidate_rat(r0)?;
        let kf = k.to_f64();
        if kf <= 0.0 {
            return None;
        }
        (0..self.insts.len())
            .all(|i| {
                let r = self.mono_val(i, m1) / self.mono_val(i, m2);
                r.is_finite() && (r - kf).abs() < 1e-9 * kf.max(1.0)
            })
            .then_some(k)
    }

    /// Stages S5/S6: products with T2 cofactors, T4 powers and T3 log bridges
    /// on the current figure.
    pub(in crate::ratio) fn prove_products_trig(
        &mut self,
        lhs: &MExpr,
        rhs: &MExpr,
        goal_text: &str,
        pts: &[PointId],
        relevant: &[PointId],
        aux_prose: &[String],
    ) -> Option<String> {
        let goal = self.homogeneous_goal(lhs, rhs)?;
        let goal_pts: BTreeSet<PointId> = goal.terms.keys().flatten().flat_map(|&k| k.points()).collect();
        self.psteps.clear();
        self.pending.clear();
        self.steps.clear();
        self.trig_cache.clear();
        self.trig_checks = 0;
        let degree = goal.terms.keys().next()?.len();
        if degree == 2 {
            let goal_segs: BTreeSet<LAtom> = goal.terms.keys().flatten().filter_map(|k| k.len()).collect();
            self.gather_generic_similar(relevant);
            self.gather_bisector_products();
            self.gather_product_relations(&goal_segs);
        }
        self.gather_center_power(pts);
        let goal_monos: BTreeSet<Mono> = goal.terms.keys().cloned().collect();
        let mut pool: BTreeSet<Mono> = goal_monos.clone();
        for s in &self.psteps {
            if let Some(e) = &s.eq {
                pool.extend(e.terms.keys().filter(|m| m.len() == degree).cloned());
            }
        }
        self.gather_cofactor_substitutions(&mut pool);
        self.gather_trig(pts, &goal_pts);
        self.gather_log_bridges(&pool, &goal_monos);
        let used = self.certified_products(&goal)?;
        if used.is_empty() {
            return None;
        }
        let val = eval_numeric(lhs, self).unwrap_or(f64::NAN);
        Some(self.prender(&used, goal_text, val, aux_prose))
    }

    /// Recompute the centred circles after auxiliary congruences were added.
    fn refresh_circles(&mut self) {
        self.circles.clear();
        for (o, set) in equal_radius_circles(self.names.len(), &self.congs, &self.abs_len) {
            let pts: Vec<PointId> = set.iter().copied().collect();
            let r2 = self.dist(0, o, pts[0]).powi(2);
            if pts.iter().all(|&p| (self.dist(0, o, p).powi(2) - r2).abs() < 1e-6 * (1.0 + r2)) {
                self.circles.push((o, set.clone()));
                if set.len() >= 4 && !self.concyclic.iter().any(|c| set.is_subset(c)) {
                    self.concyclic.push(set);
                }
            }
        }
    }

    /// Feet of perpendiculars from a goal point to a line through two goal
    /// points (the same family as the general aux search).
    fn trig_feet(&self, g: &[PointId]) -> Vec<AuxCand> {
        let kid = self.names.len() as PointId;
        let mut out = Vec::new();
        for &p in g {
            for (ai, &a) in g.iter().enumerate() {
                for &b in &g[ai + 1..] {
                    if p == a || p == b {
                        continue;
                    }
                    out.extend(self.build_cand(
                        "K".to_string(),
                        format!("Let K be the foot of the perpendicular from {} to {}.", self.nm(p), self.seg(a, b)),
                        vec![AuxFact::Coll(a, kid, b), AuxFact::Perp([p, kid, a, b])],
                        move |i| {
                            let (pa, va, vb) = (i[p as usize], i[a as usize], i[b as usize]);
                            let d = vb - va;
                            let t = (pa - va).dot(d) / d.dot(d).max(1e-18);
                            Some(va + d * t)
                        },
                    ));
                }
            }
        }
        out
    }

    /// Second meets with a concyclic set (its three first points fix the circle).
    fn trig_concyclic_meets(&self, g: &[PointId]) -> Vec<AuxCand> {
        let kid = self.names.len() as PointId;
        let mut out = Vec::new();
        for set in &self.concyclic {
            let on: Vec<PointId> = set.iter().copied().collect();
            if on.len() < 3 {
                continue;
            }
            let (o0, o1, o2) = (on[0], on[1], on[2]);
            for &v in g {
                for &r in &on {
                    if v == r {
                        continue;
                    }
                    let mut cyc = on.clone();
                    cyc.push(kid);
                    out.extend(self.build_cand(
                        "K".to_string(),
                        format!("Let K be the second meet of line {} with the circle.", self.seg(v, r)),
                        vec![AuxFact::Coll(v, r, kid), AuxFact::Cyclic(cyc)],
                        move |i| {
                            let circ = NumCircle::through(i[o0 as usize], i[o1 as usize], i[o2 as usize])?;
                            second_meet(i[v as usize], i[r as usize], circ.center, circ.r, i[r as usize])
                        },
                    ));
                }
            }
        }
        out
    }

    /// Stage S6: one auxiliary point (a circle second meet or a perpendicular
    /// foot), then [`Figure::prove_products_trig`].
    pub(in crate::ratio) fn aux_search_trig(&self, lhs: &MExpr, rhs: &MExpr, goal: &str, base_pts: &[PointId]) -> Option<String> {
        self.homogeneous_goal(lhs, rhs)?;
        let mut gnames: BTreeSet<String> = BTreeSet::new();
        collect_points_m(lhs, &mut gnames);
        collect_points_m(rhs, &mut gnames);
        let gpts: Vec<PointId> = gnames.iter().filter_map(|n| self.pt(n)).collect();
        let kid = self.names.len() as PointId;
        let name = ["N", "K", "M", "P", "Q", "R", "S", "U", "V", "W", "Z"]
            .into_iter()
            .find(|n| self.pt(n).is_none())?
            .to_string();
        let mut cands = self.trig_concyclic_meets(&gpts);
        cands.extend(self.centred_second_meets(&gpts));
        cands.extend(self.trig_feet(&gpts));
        cands.truncate(MAX_TRIG_AUX);
        for cand in cands {
            let mut f = self.lean_clone();
            f.names.push(name.clone());
            for (i, inst) in f.insts.iter_mut().enumerate() {
                inst.push(cand.coords[i]);
            }
            for fact in &cand.facts {
                f.add_fact(fact);
            }
            f.refresh_circles();
            let mut pts = base_pts.to_vec();
            pts.push(kid);
            let mut rel = gpts.clone();
            rel.push(kid);
            let intro = cand.intro.replacen("Let K ", &format!("Let {name} "), 1);
            if let Some(p) = f.prove_products_trig(lhs, rhs, goal, &pts, &rel, std::slice::from_ref(&intro)) {
                return Some(p);
            }
        }
        None
    }
}

fn rat_text(k: &Rat) -> String {
    match (k.numer_i64(), k.denom_i64()) {
        (Some(n), Some(1)) => n.to_string(),
        (Some(n), Some(d)) => format!("({n}/{d})"),
        _ => format!("{}", k.to_f64()),
    }
}

fn reduce_plain(eq: &mut LEq, rows: &[(LEq, LKey)]) {
    for (r, pivot) in rows {
        if let Some(c) = eq.terms.get(pivot).cloned() {
            let factor = &c / r.terms.get(pivot).unwrap();
            eq.sub_scaled(r, &factor);
        }
    }
}

fn in_span(goal: &LEq, rows: &[(LEq, LKey)]) -> bool {
    let mut g = goal.clone();
    reduce_plain(&mut g, rows);
    g.is_zero()
}
