## Overall Architecture
Watch agent processes to derive state

### Primary Responsibilities
0. Long running daemon
1. Watch `/proc` and Agent Logs to Monitor Agent State
2. Detect session death (look at `pidfd`)
3. Connect to WindowManager
4. Serve the Switchboard Interface
5. Terminal Interfaces
6. Activity Log
7. Configuration Options

## Functionality Checkpoints
- [ ] Start a daemon which responds to 
- [ ] Build something which can detect how many agent processes are running: claude + codex
