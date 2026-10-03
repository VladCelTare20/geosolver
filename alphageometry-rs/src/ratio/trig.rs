use super::*;

const MAX_POOL: usize = 512;
const MAX_COFACTOR_ROWS: usize = 4096;

impl Figure {
    pub(super) fn mono_text(&self, m: &[LAtom]) -> String {
        let mut parts: Vec<String> = Vec::new();
        let mut i = 0;
        while i < m.len() {
            let mut j = i;
            while j < m.len() && m[j] == m[i] {
                j += 1;
            }
            let s = self.seg(m[i].0, m[i].1);
            parts.push(match j - i {
                1 => s,
                2 => format!("{s}²"),
                3 => format!("{s}³"),
                k => format!("{s}^{k}"),
            });
            i = j;
        }
        if parts.is_empty() {
            "1".to_string()
        } else {
            parts.join("·")
        }
    }

    pub(super) fn equal_len_in_all(&self, s1: LAtom, s2: LAtom) -> bool {
        (0..self.insts.len()).all(|i| {
            let (a, b) = (self.dist(i, s1.0, s1.1), self.dist(i, s2.0, s2.1));
            a > 1e-9 && b > 1e-9 && (a - b).abs() < 1e-9 * a.max(b).max(1.0)
        })
    }

    /// Segments of the figure equal to `s` in every instance (candidates only).
    pub(super) fn congruent_candidates(&self, s: LAtom) -> Vec<LAtom> {
        let n = self.names.len() as PointId;
        let mut out = Vec::new();
        for a in 0..n {
            for b in (a + 1)..n {
                if (a, b) != s && self.equal_len_in_all(s, (a, b)) {
                    out.push((a, b));
                }
            }
        }
        out
    }

    fn cong_intro(&mut self, intros: &mut BTreeMap<(LAtom, LAtom), usize>, s1: LAtom, s2: LAtom) -> usize {
        let key = if s1 <= s2 { (s1, s2) } else { (s2, s1) };
        if let Some(&i) = intros.get(&key) {
            return i;
        }
        let intro = self.ppush(
            format!(
                "{} = {} (derived from the hypotheses).",
                self.seg(key.0 .0, key.0 .1),
                self.seg(key.1 .0, key.1 .1)
            ),
            None,
            vec![],
        );
        self.pending
            .push((intro, vec![pred("cong", &[key.0 .0, key.0 .1, key.1 .0, key.1 .1])], false));
        intros.insert(key, intro);
        intro
    }

    /// T5: second meets of a line `v r` (`v` a goal point, `r` on the circle)
    /// with a circle known by its centre and exactly three points — the
    /// circumcircle `{A,B,C}` with centre `O` never reaches `concyclic`.
    pub(super) fn centred_second_meets(&self, g: &[PointId]) -> Vec<AuxCand> {
        let kid = self.names.len() as PointId;
        let mut out = Vec::new();
        for (o, set) in &self.circles {
            if set.len() != 3 || self.concyclic.iter().any(|c| set.is_subset(c)) {
                continue;
            }
            let o = *o;
            let on: Vec<PointId> = set.iter().copied().collect();
            let a = on[0];
            for &v in g {
                for &r in &on {
                    if v == r {
                        continue;
                    }
                    let cand = self.build_cand(
                        "K".to_string(),
                        format!(
                            "Let K be the second meet of line {} with the circle ({}) through {}.",
                            self.seg(v, r),
                            self.nm(o),
                            on.iter().map(|&p| self.nm(p)).collect::<String>()
                        ),
                        vec![AuxFact::Coll(v, r, kid), AuxFact::Cong([o, a, o, kid])],
                        move |i| {
                            let rad = (i[a as usize] - i[o as usize]).norm();
                            second_meet(i[v as usize], i[r as usize], i[o as usize], rad, i[r as usize])
                        },
                    );
                    out.extend(cand);
                }
            }
        }
        out
    }

    /// T2: for every certified-candidate congruence `s1 = s2` and every pool
    /// monomial `m` containing `s1`, the row `m − m[s1→s2] = 0`; the new
    /// monomial joins the pool. Runs to a fixpoint under the pool/row caps.
    pub(super) fn gather_cofactor_substitutions(&mut self, pool: &mut BTreeSet<Mono>) {
        let mut queue: Vec<Mono> = pool.iter().cloned().collect();
        let mut equal: BTreeMap<LAtom, Vec<LAtom>> = BTreeMap::new();
        let mut intros: BTreeMap<(LAtom, LAtom), usize> = BTreeMap::new();
        let mut rows: BTreeSet<(Mono, Mono)> = BTreeSet::new();
        let mut qi = 0;
        while qi < queue.len() {
            let m = queue[qi].clone();
            qi += 1;
            let atoms: BTreeSet<LAtom> = m.iter().copied().collect();
            for s in atoms {
                if !equal.contains_key(&s) {
                    let c = self.congruent_candidates(s);
                    equal.insert(s, c);
                }
                for s2 in equal[&s].clone() {
                    let mut m2 = m.clone();
                    let pos = m2.iter().position(|x| *x == s).unwrap();
                    m2[pos] = s2;
                    m2.sort();
                    let key = if m <= m2 { (m.clone(), m2.clone()) } else { (m2.clone(), m.clone()) };
                    if !rows.insert(key) {
                        continue;
                    }
                    if rows.len() > MAX_COFACTOR_ROWS {
                        return;
                    }
                    let intro = self.cong_intro(&mut intros, s, s2);
                    let mut e = PEq::default();
                    e.add(m.clone(), Rat::one());
                    e.add(m2.clone(), -Rat::one());
                    let text = format!(
                        "{} = {}, so {} = {}.",
                        self.seg(s.0, s.1),
                        self.seg(s2.0, s2.1),
                        self.mono_text(&m),
                        self.mono_text(&m2)
                    );
                    self.ppush(text, Some(e), vec![intro]);
                    if pool.len() < MAX_POOL && pool.insert(m2.clone()) {
                        queue.push(m2);
                    }
                }
            }
        }
    }
}
