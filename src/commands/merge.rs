use crate::cli::MergeArgs;
use crate::commands::drop as drop_cmd;
use crate::diff;
use crate::error::GraftError;
use crate::merge::{self, MergeOpts};
use crate::state::State;

pub fn run(args: MergeArgs) -> Result<(), GraftError> {
    State::with_state(|state| {
        let workspace = state.require_workspace(&args.name)?.clone();

        let opts = MergeOpts {
            commit: args.commit,
            message: args.message.clone(),
            drop: args.drop,
            patch: args.patch,
            no_install: args.no_install,
        };

        if args.patch {
            let changes = diff::walk_upper(&workspace)?;
            if changes.is_empty() {
                println!("nothing to merge in '{}'", args.name);
                return Ok(());
            }
            let patch_output = merge::generate_patch(&workspace, &changes)?;
            print!("{patch_output}");

            if args.drop {
                drop_cmd::remove_workspace(state, &args.name)?;
                eprintln!("removed '{}'", args.name);
            }
            return Ok(());
        }

        let result = merge::merge_workspace(&workspace, &opts)?;

        let total = result.added + result.modified + result.deleted;
        if total == 0 {
            println!("nothing to merge in '{}'", args.name);
            return Ok(());
        }

        let mut install_ran = false;
        if !args.no_install {
            if let Some(pm) = merge::detect_package_manager(&workspace) {
                merge::run_install(&workspace.base, &pm)?;
                install_ran = true;
            }
        }

        if args.commit {
            let default_msg = format!("graft merge: applied changes from workspace {}", args.name);
            let msg = args.message.as_deref().unwrap_or(&default_msg);
            merge::git_commit(&workspace.base, msg)?;
        }

        if args.drop {
            drop_cmd::remove_workspace(state, &args.name)?;
        }

        println!(
            "merged {total} files ({} added, {} modified, {} deleted)",
            result.added, result.modified, result.deleted
        );
        if result.skipped > 0 {
            println!("skipped {} files (node_modules)", result.skipped);
        }
        if install_ran {
            println!("ran package install");
        }
        if args.drop {
            println!("removed '{}'", args.name);
        }

        Ok(())
    })
}
