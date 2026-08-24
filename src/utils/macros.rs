/// `QuasarSerialize` only accepts `#[repr(u8)]` unit enums with an explicit
/// discriminant on every variant. This supplies that layout plus the derives
/// the rest of the program expects (`Clone`, `Copy`, `Debug`, `PartialEq`, `Eq`).
///
/// ```ignore
/// quasar_enum!(pub VaultStatus, Active = 0, Paused = 1);
/// ```
macro_rules! quasar_enum {
    (
        $(#[$meta:meta])*
        $vis:vis $name:ident,
        $($variant:ident = $discriminant:expr),+ $(,)?
    ) => {
        $(#[$meta])*
        #[repr(u8)]
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, ::quasar_lang::prelude::QuasarSerialize
        )]
        $vis enum $name {
            $($variant = $discriminant),+
        }
    };
}

pub(crate) use quasar_enum;

#[cfg(test)]
mod tests {
    use super::*;
    use quasar_lang::instruction_arg::InstructionArg;

    quasar_enum!(Status, Off = 0, On = 7);

    #[test]
    fn discriminants_match_the_params() {
        assert_eq!(Status::Off as u8, 0);
        assert_eq!(Status::On as u8, 7);
    }

    #[test]
    fn round_trips_through_instruction_arg() {
        let zc = Status::On.to_zc();
        assert_eq!(Status::from_zc(&zc), Status::On);
        assert!(Status::validate_zc(&zc).is_ok());
    }

    #[test]
    fn rejects_undeclared_discriminants() {
        assert!(Status::validate_zc(&1).is_err());
        assert!(Status::validate_zc(&8).is_err());
    }
}
