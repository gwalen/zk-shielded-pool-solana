use {
    crate::{
        cpi::{HelloInstruction, InitializeInstruction},
        state::root_registry::RootRegistry,
        utils::{
            constants::{EMPTY_TREE_VALUE, ROOT_RING_BUFFER_LENGTH},
            imt_tree::ImtTree,
            flatten_array::get_array_element,
        },
    },
    quasar_test::prelude::*,
};

// Deterministic addresses keep tests independent of discovery order.
const PAYER: Pubkey = Pubkey::new_from_array([1; 32]);

#[quasar_test]
fn hello_logs_the_greeting(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));

    let outcome = test.send(HelloInstruction { payer: PAYER });
    outcome.succeeds();

    // The program only logs; assert it emitted its greeting, not just that
    // the transaction succeeded.
    let logs = outcome.logs().join("\n");
    println!("XXX logs: {logs}");
    assert!(
        logs.contains("Hello, Solana!"),
        "expected the program to log its greeting, got:\n{logs}"
    );
}

#[quasar_test]
fn initialize_writes_root_registry_in_place(test: &mut Test) {
    test.add(Wallet::new().at(PAYER));
    let (root_registry_address, root_registry_bump) =
        test.derive_pda_with_bump(RootRegistry::seeds());

    test.send(InitializeInstruction { signer: PAYER })
        .succeeds();

    let expected_imt = ImtTree::new().unwrap();
    let root_registry = test.read::<RootRegistry>(root_registry_address);

    assert_eq!(root_registry.imt.root, expected_imt.root);
    assert_eq!(root_registry.imt.frontiers, expected_imt.frontiers);
    assert_eq!(root_registry.imt.zero_values, expected_imt.zero_values);
    assert_eq!(root_registry.imt.next_leaf_idx, 0);
    assert_eq!(root_registry.last_root_idx, 0);
    assert_eq!(root_registry.bump, root_registry_bump);
    assert_eq!(
        get_array_element(&root_registry.roots_history, 0),
        expected_imt.root
    );
    for index in 1..ROOT_RING_BUFFER_LENGTH {
        assert_eq!(
            get_array_element(&root_registry.roots_history, index),
            EMPTY_TREE_VALUE
        );
    }
}
