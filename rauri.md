## Assessment and Vision

1. changes to settings and commands
- make command args configurable in settings. have a short handle configurable and a long version that always stays. update default settings generation to include new settings examples: 
  -Q, --search <package>  Search for packages
    search should only show the top 15(configurable in settings as well) matches per repo(official or aur) to avoid having to scroll alot
  add command -QA to ignore match count limit and show all matches 
  -S, --install <package>  Install package (AUR or official)
  -S, --update-aur            Update AUR packages only
  -Syu, --update-all          Update whole system (official + AUR)
    add flag "--skip-aur"to upadte update whole system but skip aur packages
  -R <package>  Remove package (also removes package folder)
  -L            List installed packages
  add command -LA to List installed System packages as well
  <AUR_URL>     Install from AUR git link

2. make sure mirrorlists get updated correctly when we use -Syu to update system and before updating only aur pacakges. maybe use logic like reflector to get best mirrors and update then always before updating system and/or aur packages 