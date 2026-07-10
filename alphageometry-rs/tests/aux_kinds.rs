//! The auxiliary-point candidate generator emits the newer construction kinds
//! (arc midpoints, centres of similitude) on figures where they apply.

use ddar::aux_search::{candidates, Kind};
use ddar::Problem;

/// Two named circles (distinct centres/radii) with two figure points on the
/// first: expect arc-midpoint candidates for that chord and both centres of
/// similitude for the circle pair.
#[test]
fn arc_midpoint_and_similitude_candidates() {
    let p = Problem::parse(
        "o@0.0_0.0 = ; a@1.0_0.0 = ; b@0.0_1.0 = ; \
         q@3.0_0.0 = ; d@3.5_0.0 = ; e@2.5_0.0 = \
         cong o a o b, cong q d q e ? cong a b a b",
    )
    .unwrap();
    let cands = candidates(&p, true);
    assert!(
        cands.iter().any(|c| c.kind == Kind::ArcMidpoint),
        "no arc-midpoint candidate emitted"
    );
    let simil: Vec<_> = cands
        .iter()
        .filter(|c| c.kind == Kind::HomothetyCenter)
        .collect();
    assert_eq!(
        simil.len(),
        2,
        "expected internal + external similitude centres, got {}",
        simil.len()
    );
    // The internal centre divides O Q in ratio r1:r2 = 1:0.5, i.e. at x = 2.
    assert!(
        simil.iter().any(|c| (c.coord.x - 2.0).abs() < 1e-9 && c.coord.y.abs() < 1e-9),
        "internal similitude centre not at the expected coordinate"
    );
    // The external centre: (Q·r1 − O·r2)/(r1 − r2) = (3·1 − 0·0.5)/0.5 = 6.
    assert!(
        simil.iter().any(|c| (c.coord.x - 6.0).abs() < 1e-9 && c.coord.y.abs() < 1e-9),
        "external similitude centre not at the expected coordinate"
    );
}
