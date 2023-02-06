# Nodi
A (WIP) tool for generating graphs between markdown files, a la Obsidian.

## TODO
- [ ] Don't clear the database every run
- [ ] Chunk parsing/referencing into worker threads
- [ ] A configuration file
	- [ ] Ignore dirs
	- [ ] Default dir for creation
- [ ] Actually generate a graphical representation of the graph (svg? html?)
- [ ] "Init" command that will initialize nodi in the current dir 
- [ ] "Create" command that will open $EDITOR for the new note
- [ ] "Watch" command that will reindex/regenerate the graph
- [ ] Move out into a lib, i.e. if people want to make their own note-taking app
