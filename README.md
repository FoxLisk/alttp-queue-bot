# get invite url

go to discord application thing
OAuth2 -> URL Generator
scopes:
    bot
        create public threads

    TODO: future will require more permissions, probably

this will generate an url LIKE

https://discord.com/api/oauth2/authorize?client_id=1010729369121083563&permissions=34359738368&scope=bot

# how to ORM

install diesel CLI:

```
cargo install diesel_cli --no-default-features --features sqlite-bundled
```

```
diesel migration generate <migration name>

... write sql ...

diesel migration run

# probably good to test it
diesel migration redo
```