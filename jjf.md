# Just fot fun



Usage:

```sh
jff --term/-t <word> # open in tui
jff --concept/-c <sentence> # open in tui
jff --list/-l <word> # output: list of terms/concepts realted into a tui and link every t/c
```



## jff Window



TUI Window

```sh
|------------------------------------------------------------|
|                                                            |
|    term/concept                                            |
|                                                            |
|                                                            |
|    texto texto texto texto texto texto texto texto         |
|    texto texto texto texto texto texto texto texto         |
|    texto texto texto texto texto texto texto texto         |
|    texto texto texto texto texto texto texto.              |
|                                                            |
|    [display image]                                         |
|                                                            |
|    texto texto texto texto texto texto texto texto         |
|    texto texto texto texto texto texto.                    |
|                                                            |
|                     related content (option list) v        |
|                     --------------------------------       |
|                                                            |
|------------------------------------------------------------|

```

the contents from markdown files, render text, formula latex, diagrams, images, etc

option list --> display related concepts or terms, in markdown fetch if the md file contains

`[more:term](/path/file.md)` or `[more:concept](/path/file.md)` 



## jff paths

```sh
/usr/local/bin/jff`

~/.jff/config.json
~/.jff/map.json
~/.jff/terms/index.json
~/.jff/terms/{a,b,c,e,f...z}
~/.jff/terms/a/abraham.md
~/.jff/concepts/index.json
~/.jff/concepts/{a,b,c,e,f...z}
~/.jff/concepts/w/water_pollution.json
```

create term/concept

```sh
jff create -c/-t file.md
```



1.- create file into right folder

2.- add map.json term/concept

```json
{
    id: 001,
    ref: wt202610112211
    term: "water",
    letter: "w",
    path: "term/w/water.md",
},
{
    id: 001,
    ref: wc202610111344
    term: "water pollution",
    letter: "w",
    path: "term/w/water.md",
},
```



ref is a unique id: 
` [firts letter of term/concept][type term or concept][date with hour (ex. 2026 10 11 22:11)]`

delete by ref, not by id

```sh
jff --delete/-d -t/-c ref <ref id>
jff -dt/-dc ref <ref id>
```

search exact term

```sh
jff -set "water"
# output if exist: ref <ref number> 
```

search exact concept

```sh
jff -sec "water pollution"
# output if exist: ref <ref number> 
```

search related term/concept and show on output terminal (based on `[more:term](/path/file.md)` or `[more:concept](/path/file.md)` )

```sh
jff -src "water pollution"
jff -srt "water"
# output if exist print a list: <term/concept> ref: <ref number> 
```

**search any term/concept**: 

```sh
jff -sc (search concpet), jff -st (search term)
```



## Editor

```sh
jff -en/-ev/-ec/-eo/-ee <ref> 
```

- `-en` edit with nano

- `-ev` edit with vim/neovim (must ask if already install both)

- `-ec` edit with visual studio code

- `-ee` edit with emacs

- `-en` edit with others: ` ~/.jff/config.json`

```json
{
    editor: {
        name: "name editor",
        path: "/path/to/editor"
    }
    
}
```

## Images

` ~/.jff/config.json`

```json
{
    images: {
        enable: true,
        mime: "jpg,png",
        exclude: "gift, webp"
    }
}
```

## Repo git

`~/.jff/config.json`

 ```json
 {
     git: {
         repo: http://domain.ext/repo.git,
         autopush: true,
         user: "username",
         password: "password or token"
     }
 }
 ```



## Colors

`~/.jff/config.json`

 ```json
{
    colors: {
        title: #FFFFF
        text: #FFF000
    }
}
 ```

## Font

`~/.jff/config.json`

```json
{
	font:{
    	    title: {
        	family: "Verdana, Geneva, sans-serif",
        	wight: 600,
        	size: "30px"  # em, pt, px
    	},
    		text: {
        	family: "Times New Roman, Times, serif",
        	wight: 300,
        	size: "10pt"
    	},
    		related: {
        	family: "Ubuntu, sans-serif",
        	wight: 300,
        	size: "10pt"
    	}
	}
}
```

## Style

`~/.jff/config.json`

```json
{
    border-style: {
        line: double/single/dotted/etc,
        border: 1px
    }
}
```







