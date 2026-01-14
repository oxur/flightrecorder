# launchctl Support

We need to help users run `fliterec` as a service on macos. Let's build a dev plan around the material below.

1. Create a plist file in `~/Library/LaunchAgents/oxur.flightrecorder.fliterec.plist`
2. Create content for that file along the following lines:

```
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>oxur.flightrecorder.fliterec</string>

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

We'll probably want to put that in an `./assets/plist` directory ...

1. Ensure the daemon can be loaded:

```shell
launchctl load ~/Library/LaunchAgents/oxur.flightrecorder.fliterec.plist
```

1. Check agent management:

```shell
# Start
launchctl start oxur.flightrecorder.fliterec

# Stop
launchctl stop oxur.flightrecorder.fliterec

# Unload (disable autostart)
launchctl unload ~/Library/LaunchAgents/oxur.flightrecorder.fliterec.plist
```

1. Provide a means of installing the binary in a preferred location for users:

```shell
./bin/fliterec install binary --path=/usr/local/bin
```

1. Provide a means of setting up launchctl for users:

```shell
fliterec install launchctl
```
