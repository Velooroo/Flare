Output assets/demo.gif

Set FontSize 21
Set Width 1000
Set Height 800
Set Theme "Molokai"
Set TypingSpeed 130ms

Type "# Flare - Deploy to edge devices"
Enter
Sleep 1s

Type "flare discover"
Enter
Sleep 3s

Type "flare sync 0"
Enter
Sleep 2.5s
Type "RaPI"
Enter
Sleep 1.5s

Type "# Now it's not be work, ofc"
Enter
Sleep 1.5s
Type "curl http://localhost:3000"
Enter
Sleep 2s

Type "flare deploy kazilsky/demo-app --device RaPI"
Enter
Sleep 2s

Type "curl http://localhost:3000"
Enter
Sleep 1s

Type "# Done!"
Enter
Sleep 1s
