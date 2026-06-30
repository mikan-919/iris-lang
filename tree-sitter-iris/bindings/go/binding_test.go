package tree_sitter_iris_test

import (
	"testing"

	tree_sitter "github.com/smacker/go-tree-sitter"
	"github.com/tree-sitter/tree-sitter-iris"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_iris.Language())
	if language == nil {
		t.Errorf("Error loading Iris grammar")
	}
}
