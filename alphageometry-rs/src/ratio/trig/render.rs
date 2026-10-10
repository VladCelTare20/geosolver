use super::*;

/// What a trig row says, for display only.
#[derive(Clone, Debug)]
pub(in crate::ratio) enum Tag {
    /// `[a, b, c]` the triangle as emitted; `(x, y, z)` the row
    /// `yz / sin x = zx / sin y`.
    Los([PointId; 3], (PointId, PointId, PointId)),
    /// `sin kx = sin ky`, with the angle relation as worded.
    EqSin(LKey, LKey, String),
}

impl Figure {
    fn los_ratio(&self, x: PointId, y: PointId, z: PointId) -> String {
        format!("{}/{}", self.seg(y, z), self.sin_text(sin_key(x, y, z)))
    }

    /// One law-of-sines statement for the rows of one triangle.
    fn los_chain(&self, tri: [PointId; 3], rows: &[(PointId, PointId, PointId)]) -> String {
        let [a, b, c] = tri;
        let chain = if rows.len() >= 2 {
            format!("{} = {} = {}", self.los_ratio(a, b, c), self.los_ratio(b, c, a), self.los_ratio(c, a, b))
        } else {
            let (x, y, z) = rows[0];
            format!("{} = {}", self.los_ratio(x, y, z), self.los_ratio(y, z, x))
        };
        format!("△{}{}{}: {chain}", self.nm(a), self.nm(b), self.nm(c))
    }

    fn collinear_all(&self, a: PointId, b: PointId, c: PointId) -> bool {
        (0..self.insts.len()).all(|k| {
            let (u, v) = (self.insts[k][b as usize] - self.insts[k][a as usize], self.insts[k][c as usize] - self.insts[k][a as usize]);
            (u.x * v.y - u.y * v.x).abs() < 1e-9 * (1.0 + u.norm() * v.norm())
        })
    }

    /// Display groups for the ordered proof steps: the law-of-sines rows of one
    /// triangle become one statement, and two of them on either side of a
    /// cevian `AX` with `sin∠AXB = sin∠AXC` become the ratio lemma (three
    /// ratio lemmas in one triangle from its three vertices are trig Ceva).
    /// Purely cosmetic: every row is still shown, and no reason is added.
    pub(in crate::ratio) fn display_groups(&self, order: &[usize]) -> Vec<(Vec<usize>, String)> {
        let mut los: BTreeMap<[PointId; 3], Vec<usize>> = BTreeMap::new();
        for &i in order {
            if let Some(Tag::Los(tri, _)) = &self.steps[i].tag {
                los.entry(*tri).or_default().push(i);
            }
        }
        let tri_key = |p: PointId, q: PointId, r: PointId| {
            let mut t = [p, q, r];
            t.sort();
            t
        };
        let mut taken: BTreeSet<usize> = BTreeSet::new();
        let mut folds: Vec<(Vec<usize>, String, [PointId; 3], PointId)> = Vec::new();
        for &i in order {
            let Some(Tag::EqSin(LKey::Sin(x1, p1, q1), LKey::Sin(x2, p2, q2), rel)) = &self.steps[i].tag else {
                continue;
            };
            if x1 != x2 || taken.contains(&i) {
                continue;
            }
            let x = *x1;
            let shared = [*p1, *q1].into_iter().find(|v| [*p2, *q2].contains(v));
            let Some(a) = shared else { continue };
            let b = if *p1 == a { *q1 } else { *p1 };
            let c = if *p2 == a { *q2 } else { *p2 };
            if b == c || !self.collinear_all(b, x, c) || !self.between(b, x, c) {
                continue;
            }
            let (t1, t2) = (tri_key(a, b, x), tri_key(a, x, c));
            let (Some(r1), Some(r2)) = (los.get(&t1), los.get(&t2)) else { continue };
            if r1.iter().chain(r2).any(|r| taken.contains(r)) {
                continue;
            }
            let rows = |rs: &Vec<usize>| -> Vec<(PointId, PointId, PointId)> {
                rs.iter()
                    .filter_map(|&r| match &self.steps[r].tag {
                        Some(Tag::Los(_, row)) => Some(*row),
                        _ => None,
                    })
                    .collect()
            };
            let (n, s) = (|p: PointId| self.nm(p), |v, p, q| self.sin_text(sin_key(v, p, q)));
            let text = format!(
                "Ratio lemma in △{a}{b}{c} with cevian {a}{x}: {b}{x}/{x}{c} = {ab}·{sba} / ({ac}·{sca}). \
                 By the law of sines in {l1} and in {l2}, and {s1} = {s2} since {rel}.",
                a = n(a),
                b = n(b),
                c = n(c),
                x = n(x),
                ab = self.seg(a, b),
                ac = self.seg(a, c),
                sba = s(a, b, x),
                sca = s(a, x, c),
                l1 = self.los_chain(t1, &rows(r1)),
                l2 = self.los_chain(t2, &rows(r2)),
                s1 = s(x, a, b),
                s2 = s(x, a, c),
            );
            let mut members = vec![i];
            members.extend(r1.iter().chain(r2).copied());
            taken.extend(members.iter().copied());
            folds.push((members, text, tri_key(a, b, c), a));
        }
        let mut ceva: BTreeMap<[PointId; 3], BTreeSet<PointId>> = BTreeMap::new();
        for (_, _, tri, apex) in &folds {
            ceva.entry(*tri).or_default().insert(*apex);
        }
        let mut groups: Vec<(Vec<usize>, String)> = Vec::new();
        for (members, mut text, tri, _) in folds {
            if ceva[&tri].len() == 3 {
                text.push_str(&format!(
                    " (With the other two ratio lemmas in △{}{}{} this is trigonometric Ceva.)",
                    self.nm(tri[0]),
                    self.nm(tri[1]),
                    self.nm(tri[2])
                ));
            }
            groups.push((members, text));
        }
        for (tri, rs) in &los {
            if rs.iter().any(|r| taken.contains(r)) {
                continue;
            }
            let rows: Vec<(PointId, PointId, PointId)> = rs
                .iter()
                .filter_map(|&r| match &self.steps[r].tag {
                    Some(Tag::Los(_, row)) => Some(*row),
                    _ => None,
                })
                .collect();
            taken.extend(rs.iter().copied());
            groups.push((rs.clone(), format!("Law of sines in {}.", self.los_chain(*tri, &rows))));
        }
        for &i in order {
            if !taken.contains(&i) {
                groups.push((vec![i], self.steps[i].text.clone()));
            }
        }
        let pos: BTreeMap<usize, usize> = order.iter().enumerate().map(|(k, &i)| (i, k)).collect();
        groups.sort_by_key(|(m, _)| m.iter().map(|i| pos[i]).min().unwrap_or(usize::MAX));
        for (m, _) in groups.iter_mut() {
            m.sort_by_key(|i| pos[i]);
        }
        groups
    }
}
