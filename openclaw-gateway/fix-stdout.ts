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
            const block = catchClause.getBlock();
            const varName = variableDecl.getName();
            
            const propAccesses = block.getDescendantsOfKind(SyntaxKind.PropertyAccessExpression);
            for (const pa of propAccesses.reverse()) {
                if (pa.getText() === `${varName}.stdout`) {
                    pa.replaceWithText(`(${varName} as any).stdout`);
                    changed = true;
                } else if (pa.getText() === `${varName}.stderr`) {
                    pa.replaceWithText(`(${varName} as any).stderr`);
                    changed = true;
                }
            }
        }
    }
    
    if (changed) {
        sourceFile.saveSync();
        console.log(`Updated ${sourceFile.getFilePath()}`);
    }
});
