AppInstaller: A simple installer that:
- Pulls down a zip file from a network location
- Extracts it to %LocalAppdata%
- Creates a shortcut to the extracted executable in the Start menu

This is a Rust migration of my C application.

Usage: Installer.exe <program_name>

Author: Trevor Hamm

Actions:
- Get program name from commandline arguments
- Find newest zip file from network folder by that name
- Check / Install / Upgrade local installer   (STEP 1)
- Download zip to %localappdata%\MyApps       (STEP 2)
- Check/fail if program is currently running
- Uninstall current version (if exists)       (STEP 3)
- Unzip file                                  (STEP 4)
- Create shortcut                             (STEP 5)
- Run app on exit                             (STEP 6)

