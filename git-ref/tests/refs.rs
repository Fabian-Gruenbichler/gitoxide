type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

mod file;
mod transaction {
    mod refedit_ext {
        use git_ref::transaction::{Change, DeleteMode, RefEdit, RefEditsExt};
        use std::convert::TryInto;

        fn named_edit(name: &str) -> RefEdit {
            RefEdit {
                change: Change::Delete {
                    previous: None,
                    mode: DeleteMode::RefAndRefLog,
                    deref: false,
                },
                name: name.try_into().expect("valid name"),
            }
        }

        #[test]
        fn reject_duplicates() {
            assert!(
                vec![named_edit("HEAD")].assure_one_name_has_one_edit().is_ok(),
                "there are no duplicates"
            );
            assert!(
                vec![named_edit("refs/foo"), named_edit("HEAD")]
                    .assure_one_name_has_one_edit()
                    .is_ok(),
                "there are no duplicates"
            );
            assert_eq!(
                vec![named_edit("HEAD"), named_edit("refs/heads/main"), named_edit("HEAD")]
                    .assure_one_name_has_one_edit()
                    .expect_err("duplicate"),
                "HEAD",
                "a correctly named duplicate"
            );
        }

        mod splitting {
            use bstr::{BString, ByteSlice};
            use git_hash::ObjectId;
            use git_ref::{
                mutable::Target,
                transaction::{Change, DeleteMode, RefEdit, RefEditsExt},
                FullName, PartialName, RefStore,
            };
            use std::{cell::RefCell, collections::BTreeMap, convert::TryInto};

            struct MockStore {
                targets: RefCell<BTreeMap<BString, Target>>,
            }

            impl MockStore {
                fn assert_empty(self) {
                    assert_eq!(self.targets.borrow().len(), 0, "all targets should be used");
                }
                fn with(edits: impl IntoIterator<Item = (&'static str, Target)>) -> Self {
                    MockStore {
                        targets: {
                            let mut h = BTreeMap::new();
                            h.extend(edits.into_iter().map(|(k, v)| (k.as_bytes().as_bstr().to_owned(), v)));
                            RefCell::new(h)
                        },
                    }
                }
            }

            impl RefStore for MockStore {
                type FindOneExistingError = std::io::Error;

                fn find_one_existing(&self, name: PartialName<'_>) -> Result<Target, Self::FindOneExistingError> {
                    self.targets
                        .borrow_mut()
                        .remove(name.as_bstr())
                        .ok_or(std::io::ErrorKind::NotFound.into())
                }
            }

            fn is_deref(edit: &RefEdit) -> bool {
                match edit.change {
                    Change::Delete { deref, .. } | Change::Update { deref, .. } => deref,
                }
            }

            fn find<'a>(edits: &'a [RefEdit], name: &str) -> &'a RefEdit {
                let name: FullName = name.try_into().unwrap();
                edits
                    .iter()
                    .find(|e| e.name.as_ref() == name.as_bstr())
                    .expect("always available")
            }

            #[test]
            fn non_symbolic_refs_are_ignored_or_if_the_deref_flag_is_not_set() -> crate::Result {
                let store = MockStore::with(Some((
                    "refs/heads/anything-but-not-symbolic",
                    Target::Peeled(ObjectId::null_sha1()),
                )));
                let mut edits = vec![
                    RefEdit {
                        change: Change::Delete {
                            previous: None,
                            mode: DeleteMode::RefAndRefLog,
                            deref: false,
                        },
                        name: "SYMBOLIC_PROBABLY_BUT_DEREF_IS_FALSE_SO_IGNORED".try_into()?,
                    },
                    RefEdit {
                        change: Change::Delete {
                            previous: None,
                            mode: DeleteMode::RefAndRefLog,
                            deref: true,
                        },
                        name: "refs/heads/anything-but-not-symbolic".try_into()?,
                    },
                    RefEdit {
                        change: Change::Delete {
                            previous: None,
                            mode: DeleteMode::RefAndRefLog,
                            deref: true,
                        },
                        name: "refs/heads/does-not-exist-and-deref-is-ignored".try_into()?,
                    },
                ];

                edits.extend_with_splits_of_symbolic_refs(&store, |_| panic!("should not be called"))?;
                assert_eq!(edits.len(), 3, "no edit was added");
                assert!(
                    !is_deref(find(&edits, "refs/heads/anything-but-not-symbolic")),
                    "the algorithm corrects these flags"
                );
                assert!(
                    is_deref(find(&edits, "refs/heads/does-not-exist-and-deref-is-ignored")),
                    "non-existing refs won't change the flag"
                );
                store.assert_empty();
                Ok(())
            }

            #[test]
            #[ignore]
            fn symbolic_refs_are_split_into_referents_handling_the_reflog() {}
        }
    }
}
