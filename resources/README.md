# Format Specifications
## Filter File
To reduce the number of false-positives, you can supply a list of words that should be excluded from the search in any case.
The default list `filter_de.txt` contains the ~1000 most common German words and all number words up to twelve. 

## Input Lists
The input format is a two-column TSV file, like this:
```
{search term}<TAB>{target label, like an URI}
```

Example (excerpt from the GBIF Backbone Taxonomy):
```
Homo sapiens sapiens	https://www.gbif.org/species/7348228
```
