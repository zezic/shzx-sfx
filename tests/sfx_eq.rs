use shzx_sfx::{SFX_EQ, SfxKind};
use strum::IntoEnumIterator;

#[test]
fn every_recipe_exposes_the_shared_sfx_output_equalizer() {
    assert_eq!(SFX_EQ.center_hz, 3_000.0);
    assert_eq!(SFX_EQ.gain_db, -9.0);
    assert!((0.5..=0.8).contains(&SFX_EQ.q));
    for kind in SfxKind::iter() {
        let recipe = kind.recipe();
        assert_eq!(
            recipe.eq, SFX_EQ,
            "{kind}: inspector EQ must match the shared audio stage"
        );
        let json = serde_json::to_value(recipe).unwrap();
        assert_eq!(json["eq"]["center_hz"], 3_000.0);
        assert_eq!(json["eq"]["gain_db"], -9.0);
    }
}
