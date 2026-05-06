import { Project, SyntaxKind } from "ts-morph";

const project = new Project({
    tsConfigFilePath: "tsconfig.json",
});

project.getSourceFiles().forEach(sourceFile => {
    let changed = false;
    
    // Find all CatchClauses
    const catchClauses = sourceFile.getDescendantsOfKind(SyntaxKind.CatchClause);
    
    for (const catchClause of catchClauses) {
        const variableDecl = catchClause.getVariableDeclaration();
        if (variableDecl) {
            const typeNode = variableDecl.getTypeNode();
            if (typeNode && typeNode.getText() === "any") {
                typeNode.replaceWithText("unknown");
                
                const block = catchClause.getBlock();
                const varName = variableDecl.getName();
                
                // Insert the errMsg type guard at the top of the block
                const insertedStmts = block.insertStatements(0, `const errMsg = ${varName} instanceof Error ? ${varName}.message : String(${varName});`);
                
                // Now, safely replace references to e.message with errMsg inside this block ONLY
                const propAccesses = block.getDescendantsOfKind(SyntaxKind.PropertyAccessExpression);
                for (const pa of propAccesses.reverse()) {
                    // Skip the newly inserted statements
                    if (insertedStmts.some(stmt => pa.getStart() >= stmt.getStart() && pa.getEnd() <= stmt.getEnd())) {
                        continue;
                    }
                    if (pa.getText() === `${varName}.message`) {
                        pa.replaceWithText("errMsg");
                    } else if (pa.getText() === `${varName}.stack`) {
                        pa.replaceWithText(`(${varName} instanceof Error ? ${varName}.stack : undefined)`);
                    }
                }
                
                changed = true;
            }
        }
    }
    
    if (changed) {
        sourceFile.saveSync();
        console.log(`Updated ${sourceFile.getFilePath()}`);
    }
});
