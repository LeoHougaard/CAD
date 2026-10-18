# Settings and search

Click Search in the header, or press Alt+C. S opens the same search near the pointer. Enter words from a command, feature name, part name, or setting. Multiple words must all match. Use Up and Down to choose a result, Enter to activate it, and Escape to close search.

Search includes installed feature and export plugins, model history, parts, settings, variables, file actions, view fitting, planes, and help. Selecting a history result opens that feature for editing when another feature is not already being edited. Selecting a part highlights it.

The feature toolbar uses installed tools and moves excess tools into More when the window is narrow. Sidebars contain model history and feature controls.

Open Settings from search or the options menu. Color and navigation changes preview immediately. Save persists them; Cancel restores the preferences from before opening Settings. Background and panel colors remain dark. Selection and hover colors default to muted turquoise and purple.

Preferences include mouse rotation, pan and zoom sensitivity, SpaceMouse speed and direction options, model edges, the feature tree, and compact buttons. They are local preferences and do not change shared geometry documents.

Files are stored under `%LOCALAPPDATA%/LeoHougaard/CAD/settings.json` on Windows and `$XDG_CONFIG_HOME/leo-hougaard-cad/settings.json` on Linux. Without XDG_CONFIG_HOME, Linux uses `~/.config/leo-hougaard-cad/settings.json`. Saving uses a unique temporary file and atomic replacement. Invalid or future settings formats are reported without overwriting the file.
