use oce_model::ParamTable;

use super::real_param;
use crate::{
    Block, DewPointTDryBulPhi, ParamDefault, ParamRule, RegistryEntry, SpecificEnthalpyTDryBulPhi,
    WetBulbTDryBulPhi,
};

const P_ATM_DEFAULT: f64 = 101_325.0;

pub(super) const SPECIFIC_ENTHALPY_PARAM_DEFAULTS: &[ParamDefault] =
    &[param_default_real!("pAtm", P_ATM_DEFAULT)];

pub(super) const ENTRIES: &[RegistryEntry] = &[
    RegistryEntry {
        class_path: "CDL.Psychrometrics.DewPoint_TDryBulPhi",
        make: make_dew_point_t_dry_bul_phi,
    },
    RegistryEntry {
        class_path: "CDL.Psychrometrics.SpecificEnthalpy_TDryBulPhi",
        make: make_specific_enthalpy_t_dry_bul_phi,
    },
    RegistryEntry {
        class_path: "CDL.Psychrometrics.WetBulb_TDryBulPhi",
        make: make_wet_bulb_t_dry_bul_phi,
    },
];

pub(super) const SPECIFIC_ENTHALPY_PARAM_RULES: &[ParamRule] =
    &[ParamRule::RealFiniteGreaterThan {
        name: "pAtm",
        min: 0.0,
    }];

fn make_dew_point_t_dry_bul_phi(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(DewPointTDryBulPhi)
}

fn make_specific_enthalpy_t_dry_bul_phi(p: &ParamTable) -> Box<dyn Block> {
    Box::new(SpecificEnthalpyTDryBulPhi {
        p_atm: real_param(p, "pAtm", P_ATM_DEFAULT),
    })
}

fn make_wet_bulb_t_dry_bul_phi(_p: &ParamTable) -> Box<dyn Block> {
    Box::new(WetBulbTDryBulPhi)
}
