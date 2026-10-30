# Security policy

Roll Model is a desktop program that runs on your own computer. It does not use the network and has no accounts. What it reads is what you give it: MusicXML scores (`.musicxml`, `.xml` and `.mxl`, which is a zip file), WAV files that replace instrument sounds, controller preset and settings text files, and MIDI from your devices.

A problem is in scope when a crafted file or MIDI message can make the program crash, hang, use unbounded memory, or read or write outside its own settings folders. A wrong grade or a score that is drawn badly is an ordinary issue.

## Supported versions

The latest release and the `develop` branch.

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's private reporting: on the repository's **Security** tab choose **Report a vulnerability**. If that is not offered, write to the address in the copyright line of [LICENSE](LICENSE).

This is a spare-time project, so there is no guaranteed response time. I will answer as soon as I can, fix the problem in a patch release and, if you wish, name you in the changelog.
