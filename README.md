# Database server

I love postgres but it's way too complicated to set up and often way overkill for the stuff I wanna do with it. 
I want a database that uses very little resources, is file-backed, I can use with my Obsidian notebook and is web-accessible with good security. 
I'm sure there's a solution out there, but I've always wanted to build database, so why not.

## How

Well I have a nextcloud server which provides OAuth2.0 access. 
This DB server therefore implements authentication against an oauth2 provider which it uses to establish identity. 
Once there, it queries all databases owned by that user and exposes them via a token-based system for the user to query as desired.

## How does querying work

A database is just a collection of files referred to as pages. 
Using adapters, each page can be mapped to a number of objects. 
For example, we may have a table adapter which reads page contents as CSV and exposes relational features, while a document adapter treats each page as a separate objet, allowing for streaming and chunking etc. 
I also plan to add an adapter for rotated content, so ideal for logging. 

Pages don't necessarily have to be represented by files either. 
In a previous project, I attempted to create a binary format for small-scale data storage, where pages are pieced-together chunks scattered across a binary file.
This would allow for a network-block-device-backed database format, similar to remote file-systems, and is something I would love to try and replicate in future.

## Relational databases

Since a CSV file can represent tables, I would like to define a format which allows me to make complex queries on tabular data. 
I will try to define adapters in such a way that the adapter responsible for mapping the CSV page into its constituent objects, also implements the query language for accessing this data. 
Of course the aim of this project is not the most powerful database in existence - rather the opposite. 
I want something small and lightweight, so focusing only on information which will be truly useful to me and my projects. 

# Usage

This projec is nowhere near ready yet. But if you're curious, setup is fairly straightforward. - 'tis but a single config file!

1. Create a directory somewhere on your system where you would like all databases stored. For user-local storage I like `~/.local/state/db/`
2. Inside here create a file called `index.json` and populate it with the following structure:
```json
{
  "databases": [],
  "apps": [],
  "users": [],
  "oauth_settings": {
    "client_id": "",
    "client_secret": "",
    "redirect": "",
    "authorisation": "",
    "token": ""
  }
}
```
3. Fill in your OAuth2 information from your chosen provider. Mine is my nextcloud server, but you can use GitHub, Google, Facebook or any you'd like. Consult their docs on how to do that.
4. Build the project
```bash
git clone https://github.com/J-Cake/database-server.git
cd database-server
cargo run --package simple-database-server --bin simple-database-server -- --database $THE_PATH_WHERE_YOUR_DATABASES_WILL_LIVE # Provide the parent directory of the `index.json` file you just created.
```
6. Log in to the database under [Portal](https://localhost:2003/portal/index.html)
7. Create a new database
8. _I haven't gotten that far yet. Come back soon once I've figured out exactly how to interface with the DB_
