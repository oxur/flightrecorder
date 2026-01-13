# launchctl Support

We need to help users run `fliterec` as a service on macos. Let's build a dev plan around the material below.

1. Create a plist file in `~/Library/LaunchAgents/org.codeberg.oxur.flightrecorder.fliterec.plist`
2. Create content for that file along the following lines:

```
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>org.codeberg.oxur.flightrecorder.fliterec</string>

    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/fliterec deamon</string>
    </array>

    <key>RunAtLoad</key>
    <true/>

    <key>KeepAlive</key>
    <true/>

    <key>StandardOutPath</key>
    <string>/tmp/oxur.flightrecorder.fliterec.log</string>

    <key>StandardErrorPath</key>
    <string>/tmp/oxur.flightrecorder.fliterec.err</string>
</dict>
</plist>
```

3. Ensure the daemon can be loaded:

```
launchctl load ~/Library/LaunchAgents/org.codeberg.oxur.flightrecorder.fliterec.plist
```

4. Check agent management:

```
# Start
launchctl start org.codeberg.oxur.flightrecorder.fliterec

# Stop
launchctl stop org.codeberg.oxur.flightrecorder.fliterec

# Unload (disable autostart)
launchctl unload ~/Library/LaunchAgents/org.codeberg.oxur.flightrecorder.fliterec.plist
```
