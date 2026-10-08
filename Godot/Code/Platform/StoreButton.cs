using Deuteros.Code;
using Deuteros.Code.Objects;
using Godot;
using System;
using System.Reflection;
using System.Xml;

namespace Deuteros.Code.Platform
{
	public partial class StoreButton : ButtonAdapter<Item>
	{
		public StoreButton()
		{
		}

		public override void Redraw(bool dayPassed)
		{
			if (ObjectData != null)
			{
				HoverText = ObjectData.FullName;

				if (Selected)
				{
					AnimationState = Enums.SidePanel_Button_State_Animations.Red;
				}
				else
				{
					AnimationState = Enums.SidePanel_Button_State_Animations.Static_Green;
				}
			}
			else if (ObjectData == null)
			{
				AnimationState = Enums.SidePanel_Button_State_Animations.Static_Locked;
			}

			if (AnimatedSprite != null && AnimatedSprite.Animation.ToString() != AnimationState.ToString())
				AnimatedSprite.Play(AnimationState.ToString());
		}

		public override void MouseClickedMe()
		{
			if (ObjectData != null)
				EmitSignal("Clicked", ObjectData.Research.ResearchOrder);
		}
	}
}
