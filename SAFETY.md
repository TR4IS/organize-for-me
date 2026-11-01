# Safety

People lost games and files to an earlier app of mine, so these promises come first.
A change that weakens one needs a very good reason and a new test.
All tests named here are in `crates/ledger-platform/tests/safety.rs` unless noted.

## The promises

| Promise | How the code keeps it | Tests |
|---|---|---|
| Never overwrites a file | `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING` | `a_taken_name_is_never_replaced`, `a_name_differing_only_by_letter_case_is_not_replaced`, `many_files_with_one_name_all_survive` |
| Never deletes a file | No delete call exists, and `clippy.toml` fails the build on one | Linter, run on every change |
| Never copies across drives | `MoveFileExW` without `MOVEFILE_COPY_ALLOWED` | `another_drive_is_refused_and_the_file_stays` |
| Keeps duplicates | A taken name gets `name (1)`, without comparing contents | `exact_duplicates_are_kept` |
| Every move can be put back | The journal records it, and the undo cannot replace a file either | `void_puts_the_file_back`, `void_never_replaces_a_new_file_in_the_original_spot`, `a_void_can_be_undone_after_restarting_the_app` |
| A crash loses nothing | The intent reaches the disk before the move, and recovery checks the disk | `crash_before_the_rename_finds_the_file_unmoved`, `crash_after_the_rename_finds_the_file_moved_and_undoable`, `a_half_written_journal_line_is_ignored_and_later_writes_are_safe` |
| No record, no move | A failed journal write stops the move before it starts | `no_move_starts_if_the_journal_cannot_be_written` |
| Stays out of games and system folders | The folder guard, checked again before each item | `a_folder_holding_game_files_is_refused_and_nothing_moves`, `a_canonical_path_to_windows_is_blocked`, plus `guard.rs` |
| Leaves fragile items alone | Shortcuts, hidden files, unfinished downloads, links and files in use | `shortcuts_and_unfinished_downloads_stay`, `hidden_files_stay`, `a_link_is_never_moved`, `a_file_being_written_is_not_moved` |
| Does not sort disguised programs | They are held until the user decides, and learned choices cannot clear them | `a_disguised_program_is_held_where_it_is`, plus `judge.rs` |

## What is not promised

- **Programs that expect a file in its old place can still break.** Moving is the
  app's job. The guard reduces this risk for known places but cannot detect every one.
- **The guard cannot recognize every game.** It knows common game folder names and a
  few game files. An unusual game in an ordinary folder gets through.
- **The disk itself can fail.** The app cannot protect against failing hardware.
- **File type checks are not antivirus.** Holding a disguised program is a warning,
  and a file that reaches `exe/` is not proven safe.
- **The journal assumes one running app.** Stopping a second copy is the app's job,
  and that app does not exist yet.

## Not tested on real hardware

- **A full disk:** the error is mapped and unit tested, but no test fills a real drive.
- **A network folder or an unplugged drive:** these are untested so far.
- **Real power loss:** crash tests recreate the journal and disk as they would stand
  after a crash, without cutting power.
