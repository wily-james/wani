# wani

## A familiar [WaniKani](https://www.wanikani.com/) experience -- in the terminal

- Do your [WaniKani](https://www.wanikani.com/) lessons and reviews from the command line.
- Work offline, syncing reviews with the WaniKani servers later.
- Familiar UI and keybindings, but even more keyboard-centric.
- Minimizes network requests for a more lag-free experience.

## DISCLAIMER
This is an unofficial WaniKani client. Use at your own risk.

<!--toc:start-->
  - [INSTALL](#install)
    - [From Source](#from-source)
    - [Binaries](#binaries)
  - [RECOMMENDED SETUP](#recommended-setup)
  - [USE](#use)
  - [HOTKEYS](#hotkeys)
  - [CONFIGURATION](#configuration)
<!--toc:end-->

![demo](https://raw.githubusercontent.com/wily-james/wani/main/demo.gif)

## INSTALL

### From Source
With [rust/cargo](https://www.rust-lang.org/tools/install) installed:
```
cargo install --git https://github.com/wily-james/wani.git
```

### Binaries

TODO

## RECOMMENDED SETUP

You'll need to provide your WaniKani personal [API token](https://www.wanikani.com/settings/personal_access_tokens).  
The easiest way to set this up is to create a config file at "~/.config/wani/.wani.conf".  
Add the following line to that file (replacing your_auth_token with your actual API token).
```
auth: your_auth_token
```

For more information about the wani config file, see [CONFIGURATION](#configuration).

Next, run this command to sync wani with the WaniKani servers:
```
wani sync
```
This can take awhile, but when it's finished, wani should be ready to use online or offline!

## USE

Get your wanikani summary:
```
wani
```

Check the help:
```
wani -h
```

First-time setup and download subjects:
```
wani sync
```

Do your reviews:
```
wani r
```

Do your lessons:
```
wani l
```

You can view the review or lesson help by typing '?' during a lesson or review session.  
You can quit a review session at any time (with ctrl+c), and your progress will be saved locally and/or uploaded to WaniKani servers.  

## HOTKEYS

wani hotkeys are similar to those of the WaniKani web application.  
View the help page during a review or lesson session by typing '?' (a question mark).

## CONFIGURATION

### FILE PATH

Wani looks for a config file by default at "~/.config/wani/.wani.conf"  
The containing directory for the ".wani.conf" file can be overridden by specifying the directory path as a command line argument:
```
wani -c /some/path
```

Or by adding the desired path to the WANI_CONFIG_PATH environment variable
```
export WANI_CONFIG_PATH=/some/path
```

### CONFIG OPTIONS

Sample .wani.conf file:

```
auth: your_auth_token
colorblind: true
datapath: /some/path
```

#### Options (all are optional):
Specify your WaniKani personal API token. See https://www.wanikani.com/settings/personal_access_tokens
```
auth: your_auth_token
```
  

Enable some minimal accessibility features for red-green colorblindness.
```
colorblind: true
``` 
  
Specify an alternate location for your local cache of WaniKani subject data.
```
datapath: /some/path
``` 
