# Contributing to Documentation

The documentation for Pywr V2 is located in the pywr-next repository, [here](https://github.com/pywr/pywr-next) in the `pywr-book` subfolder.

The documentation is written using 'markdown', a format which enables easy formatting for the web. 

This website can help get started: [www.markdownguide.org](https://www.markdownguide.org)

To contribute documentation for Pywr V2, we recommend following the steps below to ensure we can review and integrate any changes as easily as possible.

## Steps to create documentation

1. Fork the pywr-next repository

![Fork the repository](./images/making_documentation/fork.png "Fork")

2. Clone the fork

``` <bash>
    git clone https://github.com/MYUSER/pywr-next
```

3. Create a branch

``` <bash>
    git checkout -b my-awesome-docs
```

4. Open the book documentation in your favourite editor

``` <bash>
    vi pywr-next/pywr-book/introduction.md
```

Which should look something like this:

![An example docs file](./images/making_documentation/docs_example.png "Docs example")

5. Having modified the documentation, add and commit the changes <ins>using the commit format<ins>

```<bash>
git add introduction.md"
```

```<bash>
git commit -m "docs: Add an example documentation"
```

6. Create a pull request from your branch 
  1. In your fork, click on the 'Pull Requests' tab
    ![Pull request](./images/making_documentation/pr1.png "Pull Request")

  2. Click on 'New Pull Request'
    ![Pull request](./images/making_documentation/pr2.png "Pull Request")

  3. Choose your branch from the drop-down on the right-hand-side
    ![Pull request](./images/making_documentation/pr3.png "Pull Request")

  4. Click 'Create Pull Request' when the button appears 
    ![Pull request](./images/making_documentation/pr4.png "Pull Request")

  5. Add a note if you want, and click 'Create Pull Request'
    ![Pull request](./images/making_documentation/pr5.png "Pull Request")

